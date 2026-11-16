"""Tests for check-postgres-image.py, the guard that every Postgres start goes through one home.

    python3 -m unittest discover -s .github/scripts -p 'test_*.py'

The scanner runs over small fixture trees written to a temporary directory, and once over
this repository: a test or bench that starts Postgres on its own fails CI here (issue #342).
"""
import importlib.util
import pathlib
import re
import subprocess
import sys
import tempfile
import textwrap
import unittest

HERE = pathlib.Path(__file__).resolve().parent
SCRIPT = HERE / "check-postgres-image.py"
REPO = HERE.parents[1]

spec = importlib.util.spec_from_file_location("check_postgres_image", SCRIPT)
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)


class Findings(unittest.TestCase):
    def lines(self, source: str) -> list[int]:
        return [line for line, _ in guard.findings_in(textwrap.dedent(source))]

    def test_a_bare_default_start_is_caught(self) -> None:
        self.assertEqual(self.lines("let c = Postgres::default().start().await.unwrap();\n"), [1])

    def test_a_tag_is_caught_on_its_own_line(self) -> None:
        source = """\
            let c = Postgres::default()
                .with_tag("16-alpine")
                .start();
        """
        self.assertEqual(self.lines(source), [1, 2])

    def test_line_breaks_inside_a_match_do_not_hide_it(self) -> None:
        source = """\
            let c = testcontainers_modules::postgres::Postgres
                ::default()
                .
                with_tag
                ("11-alpine");
        """
        self.assertEqual(self.lines(source), [1, 1, 3])

    def test_a_tag_on_the_home_image_is_caught(self) -> None:
        self.assertEqual(self.lines('let c = test_postgres::image().with_tag("11-alpine");\n'), [1])

    def test_a_generic_postgres_image_is_caught(self) -> None:
        self.assertEqual(self.lines('GenericImage::new(\n    "postgres", "16")\n'), [1])

    def test_an_aliased_import_is_caught(self) -> None:
        source = """\
            use testcontainers_modules::postgres::Postgres as Pg;
            let c = Pg::default();
        """
        self.assertEqual(self.lines(source), [1])

    def test_the_default_trait_is_caught_through_the_import(self) -> None:
        source = """\
            use testcontainers_modules::postgres::Postgres;
            let a: Postgres = Default::default();
            let b = <Postgres as Default>::default();
        """
        self.assertEqual(self.lines(source), [1])

    def test_a_nested_glob_or_renamed_import_is_caught(self) -> None:
        source = """\
            use testcontainers_modules::{postgres::Postgres, testcontainers::ImageExt};
            use testcontainers_modules::{postgres, testcontainers};
            use testcontainers_modules::*;
            use testcontainers_modules as tm;
            extern crate testcontainers_modules as tcm;
            let p = testcontainers_modules :: postgres :: Postgres::default();
        """
        self.assertEqual(self.lines(source), [1, 2, 3, 4, 5, 6, 6])

    def test_a_comment_between_the_type_and_its_default_is_caught(self) -> None:
        self.assertEqual(self.lines("let c = Postgres /* the image */ ::default();\n"), [1])

    def test_a_postgres_generic_image_is_caught_in_any_spelling(self) -> None:
        source = """\
            GenericImage::new("docker.io/library/postgres", "16");
            GenericImage::new("Postgres", "16");
            let name = "postgres";
            GenericImage::new(name, "16");
            use testcontainers_modules::testcontainers::GenericImage as Image;
        """
        self.assertEqual(self.lines(source), [1, 2, 4, 5])

    def test_a_tag_or_name_through_the_trait_is_caught(self) -> None:
        source = """\
            let c = ImageExt::with_tag(test_postgres::image(), "11-alpine");
            let d = GenericImage::new("opensearchproject/opensearch", "2").with_name("postgres");
        """
        self.assertEqual(self.lines(source), [1, 2])

    def test_comments_and_strings_never_count(self) -> None:
        source = """\
            let a = 1; // Postgres::default() starts 11-alpine
            /* use testcontainers_modules::postgres::Postgres; */
            /* nested /* Postgres::default() */ still a comment */
            let s = "// not a comment"; let t = test_postgres::image();
            let u = r#"Postgres::default() in a raw string"#;
            let q = '"'; let v = test_postgres::image();
            fn f<'a>(x: &'a str) {}
        """
        self.assertEqual(self.lines(source), [])

    def test_the_home_and_other_images_pass(self) -> None:
        source = """\
            let c = test_postgres::image().with_cmd(["postgres"]).start();
            let p = test_postgres::pinned(Pinned::ByteaMinMax).start();
            let o = GenericImage::new("opensearchproject/opensearch", "2");
            let b = GenericImage::new("edoburu/pgbouncer", "v1");
            let t: test_postgres::Container = start().await;
            use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};
            use {sqlx as _, testcontainers_modules as _};
        """
        self.assertEqual(self.lines(source), [])

    def test_a_comment_line_is_not_a_start(self) -> None:
        source = """\
            /// `Postgres::default()` starts 11-alpine, so this crate never calls it.
            // .with_tag("16-alpine")
        """
        self.assertEqual(self.lines(source), [])


class Tree(unittest.TestCase):
    def setUp(self) -> None:
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.root = pathlib.Path(tmp.name)

    def write(self, relative: str, text: str) -> None:
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")

    def run_guard(self) -> subprocess.CompletedProcess:
        return subprocess.run([sys.executable, str(SCRIPT), str(self.root)], capture_output=True, text=True,
                              check=False)

    def test_only_the_home_may_name_the_image(self) -> None:
        self.write("dev/test-postgres/src/lib.rs", 'Postgres::default().with_tag("16-alpine")\n')
        self.write("libs/a/tests/ok.rs", "test_postgres::image().start()\n")
        self.write("libs/a/target/debug/build/gen.rs", "Postgres::default()\n")
        self.assertEqual(self.run_guard().returncode, 0)

    def test_a_start_anywhere_else_fails_with_its_location(self) -> None:
        self.write("dev/test-postgres/src/lib.rs", "")
        self.write("libs/a/benches/b.rs", "\n\nPostgres::default().start()\n")
        self.write("apps/c/tests/common/mod.rs", 'image()\n    .with_tag("14-alpine")\n')
        self.write("sdk/d/tests/t.rs", "Postgres::default()\n")
        self.write("docs/elsewhere.rs", "Postgres::default()\n")
        result = self.run_guard()
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn("libs/a/benches/b.rs:3:", result.stdout)
        self.assertIn("apps/c/tests/common/mod.rs:2:", result.stdout)
        self.assertIn("sdk/d/tests/t.rs:1:", result.stdout)
        self.assertNotIn("docs/", result.stdout)


class ReviewAbDefault(unittest.TestCase):
    """`review-ab.sh --pg` defaults to the home's version, read from its source."""

    def help_text(self, script: pathlib.Path) -> subprocess.CompletedProcess:
        return subprocess.run(["bash", str(script), "--help"], capture_output=True, text=True, check=False)

    def test_the_default_version_is_the_homes(self) -> None:
        home = (REPO / "dev/test-postgres/src/lib.rs").read_text(encoding="utf-8")
        tag = re.search(r'^pub const DEFAULT_TAG: &str = "(\d+)-alpine";$', home, re.M)
        self.assertIsNotNone(tag, "dev/test-postgres has no DEFAULT_TAG of the form <major>-alpine")
        result = self.help_text(HERE / "review-ab.sh")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(f"[--pg {tag.group(1)}]", result.stdout)

    def test_a_home_without_the_constant_fails_clearly(self) -> None:
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        script = pathlib.Path(tmp.name) / ".github/scripts/review-ab.sh"
        script.parent.mkdir(parents=True)
        script.write_text((HERE / "review-ab.sh").read_text(encoding="utf-8"), encoding="utf-8")
        result = self.help_text(script)
        self.assertEqual(result.returncode, 2)
        self.assertIn("dev/test-postgres/src/lib.rs", result.stderr)


class Repository(unittest.TestCase):
    def test_every_postgres_start_goes_through_the_home(self) -> None:
        report = guard.scan(REPO)
        self.assertEqual(report, [], "\n" + "\n".join(report))


if __name__ == "__main__":
    unittest.main()
