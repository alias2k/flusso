//! The memory this process may use, read once at start, so `run` can warn
//! when `[concurrency] max_built_mib` alone reaches it.
//!
//! Linux only, from files, and best effort:
//!
//! - the cgroup v2 limit: the least `memory.max` on the way from this
//!   process's cgroup (`/proc/self/cgroup`) up to the root;
//! - else cgroup v1's `memory/memory.limit_in_bytes`;
//! - else the host's `MemTotal` (`/proc/meminfo`).
//!
//! Elsewhere, or when nothing is readable, nothing is known and nothing is
//! said: the warning is a hint, never a refusal.

/// The most memory the process may use, and where that was read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ceiling {
    /// A cgroup limit, in bytes.
    Cgroup(u64),
    /// The host's memory, in bytes.
    Host(u64),
}

/// The ceiling of this process, if one can be read.
pub(crate) fn ceiling() -> Option<Ceiling> {
    ceiling_with(|path| std::fs::read_to_string(path).ok())
}

fn ceiling_with(read: impl Fn(&str) -> Option<String>) -> Option<Ceiling> {
    let v2 = read("/proc/self/cgroup").and_then(|cgroups| {
        let path = cgroups.lines().find_map(|line| line.strip_prefix("0::"))?;
        let mut least: Option<u64> = None;
        let mut dir = path.trim().trim_end_matches('/').to_owned();
        loop {
            if let Some(bytes) = read(&format!("/sys/fs/cgroup{dir}/memory.max"))
                .as_deref()
                .and_then(cgroup_limit)
            {
                least = Some(least.map_or(bytes, |least| least.min(bytes)));
            }
            match dir.rfind('/') {
                Some(cut) => dir.truncate(cut),
                None => break,
            }
        }
        least
    });
    let v1 = || {
        read("/sys/fs/cgroup/memory/memory.limit_in_bytes")
            .as_deref()
            .and_then(cgroup_limit)
    };
    v2.or_else(v1).map(Ceiling::Cgroup).or_else(|| {
        read("/proc/meminfo")
            .as_deref()
            .and_then(mem_total)
            .map(Ceiling::Host)
    })
}

/// A cgroup limit file's bytes; `None` for no limit (v2 writes `max`, v1 a
/// number near 2^63).
fn cgroup_limit(text: &str) -> Option<u64> {
    let bytes: u64 = text.trim().parse().ok()?;
    (bytes < 1 << 60).then_some(bytes)
}

fn mem_total(text: &str) -> Option<u64> {
    text.lines()
        .find_map(|line| line.strip_prefix("MemTotal:"))?
        .trim()
        .strip_suffix("kB")?
        .trim()
        .parse::<u64>()
        .ok()?
        .checked_mul(1024)
}

/// The warning to log when a budget of `budget` bytes alone reaches the
/// ceiling: a stalled sink fills the budget, and the process would be killed
/// before ingest is paced.
pub(crate) fn over_budget(budget: usize, ceiling: Option<Ceiling>) -> Option<String> {
    let (bytes, what) = match ceiling? {
        Ceiling::Cgroup(bytes) => (bytes, "the cgroup's memory limit"),
        Ceiling::Host(bytes) => (bytes, "the host's memory"),
    };
    (budget as u64 >= bytes).then(|| {
        format!(
            "[concurrency] max_built_mib ({} MiB) reaches {what} ({} MiB): a sink that \
             stops acknowledging fills the budget, and the process runs out of memory \
             before ingest is paced; lower max_built_mib",
            budget >> 20,
            bytes >> 20
        )
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn files(entries: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let files: HashMap<String, String> = entries
            .iter()
            .map(|(path, text)| ((*path).to_owned(), (*text).to_owned()))
            .collect();
        move |path| files.get(path).cloned()
    }

    const MEMINFO: &str = "MemTotal:       16384000 kB\nMemFree:  1 kB\n";

    #[test]
    fn the_least_v2_limit_up_the_tree_wins() {
        let read = files(&[
            ("/proc/self/cgroup", "0::/a/b/c\n"),
            ("/sys/fs/cgroup/a/b/c/memory.max", "max\n"),
            ("/sys/fs/cgroup/a/b/memory.max", "1073741824\n"),
            ("/sys/fs/cgroup/a/memory.max", "2147483648\n"),
            ("/proc/meminfo", MEMINFO),
        ]);
        assert_eq!(ceiling_with(read), Some(Ceiling::Cgroup(1 << 30)));
    }

    #[test]
    fn a_container_at_its_namespace_root_reads_the_root_file() {
        let read = files(&[
            ("/proc/self/cgroup", "0::/\n"),
            ("/sys/fs/cgroup/memory.max", "536870912\n"),
        ]);
        assert_eq!(ceiling_with(read), Some(Ceiling::Cgroup(512 << 20)));
    }

    #[test]
    fn without_a_cgroup_limit_the_host_is_the_ceiling() {
        let unlimited_v1 = files(&[
            (
                "/sys/fs/cgroup/memory/memory.limit_in_bytes",
                "9223372036854771712\n",
            ),
            ("/proc/meminfo", MEMINFO),
        ]);
        assert_eq!(
            ceiling_with(unlimited_v1),
            Some(Ceiling::Host(16_384_000 * 1024))
        );
        let v1 = files(&[("/sys/fs/cgroup/memory/memory.limit_in_bytes", "268435456\n")]);
        assert_eq!(ceiling_with(v1), Some(Ceiling::Cgroup(256 << 20)));
        assert_eq!(ceiling_with(files(&[])), None);
    }

    #[test]
    fn only_a_budget_reaching_the_ceiling_warns() {
        assert_eq!(over_budget(512 << 20, None), None);
        assert_eq!(over_budget(512 << 20, Some(Ceiling::Cgroup(1 << 30))), None);
        let warning = over_budget(512 << 20, Some(Ceiling::Cgroup(512 << 20))).unwrap();
        assert!(
            warning.contains("cgroup's memory limit (512 MiB)"),
            "{warning}"
        );
        assert!(
            over_budget(1 << 40, Some(Ceiling::Host(64 << 30)))
                .unwrap()
                .contains("the host's memory (65536 MiB)")
        );
    }
}
