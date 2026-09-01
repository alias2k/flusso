# Lib adoptions

The workspace releases as three independent trains (libs / cli+design /
query — see `release-plz.toml`), so a libs release does **not** ship new
binaries by itself. To get a libs fix into the docker image and dist
installers, cut a cli-train release by appending one line here in a
`fix(cli): adopt <crate> <version> (<reason>)` commit — release-plz counts
the touched file as a cli change, bumps the cli train, and the
`flusso-cli-v*` tag drives docker + dist. The binaries always embed
main-tip libs (path deps), so the appended line is both the trigger and the
audit trail of which lib fixes were deliberately shipped.

## Log

<!-- newest first: `- YYYY-MM-DD: <crate> <version> (<reason>)` -->
