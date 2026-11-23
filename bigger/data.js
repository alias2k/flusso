window.BENCHMARK_DATA = {
  "lastUpdate": 1795473834082,
  "repoUrl": "https://github.com/alias2k/flusso",
  "entries": {
    "flusso (bigger is better)": [
      {
        "commit": {
          "author": {
            "email": "jakub.wasilczyk@alias2k.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "distinct": true,
          "id": "1d6d96c06a9dd346c5f3b6403318687c9d951b67",
          "message": "ci(bench): tolerate a PR base without the in-process benches",
          "timestamp": "2026-09-05T18:13:38+02:00",
          "tree_id": "172f8ad4968a6f4c24e3ccede5ea272eb1b8b04e",
          "url": "https://github.com/alias2k/flusso/commit/1d6d96c06a9dd346c5f3b6403318687c9d951b67"
        },
        "date": 1788626130640,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 983.2763657585565,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 846.3027298900764,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 667.7251241858594,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 834.3739336984812,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "jakub.wasilczyk@alias2k.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "distinct": true,
          "id": "618579922d0af853ea6d3f088a087045f060287b",
          "message": "ci(bench): dispatch the Pages deploy after storing points\n\nA push made with the workflow token does not start other workflows, so the\nbenchmarks-branch trigger in pages.yml never fired from the bench run and\n/bench/ stayed 404 until the next main push. Dispatch pages.yml explicitly.",
          "timestamp": "2026-09-06T15:26:34+02:00",
          "tree_id": "fd3f039b802b77d546fb26360b1e9ce31a06ee8b",
          "url": "https://github.com/alias2k/flusso/commit/618579922d0af853ea6d3f088a087045f060287b"
        },
        "date": 1788702244962,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 768.2937850208403,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 498.83746001915415,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 485.7201157362836,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 502.0640946773954,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "jakub.wasilczyk@alias2k.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "distinct": true,
          "id": "7eef16156dab0533610a448c5bb7ad8667a3233d",
          "message": "docs(context): quiet stream and fence — the terms #135 and #136 need",
          "timestamp": "2026-09-06T15:35:39+02:00",
          "tree_id": "63fb4dc297ff3af6840d1a0faedcc5c8b27aaf08",
          "url": "https://github.com/alias2k/flusso/commit/7eef16156dab0533610a448c5bb7ad8667a3233d"
        },
        "date": 1788703254422,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 748.0675201674217,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 630.7613220206567,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 581.5743493614881,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 636.5400461740799,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "jakub.wasilczyk@alias2k.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "distinct": true,
          "id": "2122a5794699367b914e7f57791b3729f9654d60",
          "message": "docs: name the deadline check and its guard in the batching invariant",
          "timestamp": "2026-09-07T10:19:27+02:00",
          "tree_id": "ba8c72eb319363bcab57e336ef13e399ab0c21fd",
          "url": "https://github.com/alias2k/flusso/commit/2122a5794699367b914e7f57791b3729f9654d60"
        },
        "date": 1788770201371,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 695.5336281291641,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 472.4224076372491,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 451.23528243635667,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 477.83507142899117,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "3001bb179bfb87fb1ef841a626b29333a4afefe9",
          "message": "Merge pull request #142 from alias2k/fix/child-delete-preimage\n\nfix(source): trace deleted and re-parented children through the WAL pre-image",
          "timestamp": "2026-10-08T11:29:11+02:00",
          "tree_id": "a4df32b932d6a54d46c5e24ac30893b6c83f8f9b",
          "url": "https://github.com/alias2k/flusso/commit/3001bb179bfb87fb1ef841a626b29333a4afefe9"
        },
        "date": 1791453262198,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 672.1368549249754,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 460.3261697565994,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 438.5761096816223,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 454.1348660057017,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "9d77d6eb02798d7fcb1664d948efc39ac8667262",
          "message": "Merge pull request #146 from alias2k/feat/aggregate-through-semantics\n\nfeat(schema): distinct + junction filters on aggregates over through; ids honours filters",
          "timestamp": "2026-10-09T09:51:11+02:00",
          "tree_id": "fe2d02f341a8c5c1a554391a0be250296a740179",
          "url": "https://github.com/alias2k/flusso/commit/9d77d6eb02798d7fcb1664d948efc39ac8667262"
        },
        "date": 1791533317742,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 625.0954890008727,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 464.657059582915,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 431.6510711541246,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 451.5772146459326,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "3fbf901c29cb48439ba266450bcfc4d058ed137e",
          "message": "Merge pull request #107 from alias2k/release-plz-2026-08-17T11-30-46Z",
          "timestamp": "2026-11-06T12:09:05+01:00",
          "tree_id": "f3b73f6098bf9da301ed9fcf8457ddc545c6ed3f",
          "url": "https://github.com/alias2k/flusso/commit/3fbf901c29cb48439ba266450bcfc4d058ed137e"
        },
        "date": 1793964446642,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 1258.3037976349124,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 919.8392160720183,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 900.1967606114897,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 935.2217797757163,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "75a01aed5b629d9c5c265bc1c347252fb42a7ddb",
          "message": "Merge pull request #149 from alias2k/fix/validate-hook-runner\n\nfix(plugin): validate hook prefers the workspace build over a PATH flusso",
          "timestamp": "2026-11-10T10:52:36+01:00",
          "tree_id": "27d0b9e42f93fbf006838550168ecf85f9e0e22c",
          "url": "https://github.com/alias2k/flusso/commit/75a01aed5b629d9c5c265bc1c347252fb42a7ddb"
        },
        "date": 1794305431824,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 627.0644425251486,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 406.5362578748606,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 412.2173223882099,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 409.57947257231314,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "c12abf0b799970a6e4195814b787bd1bf18274ed",
          "message": "Merge pull request #152 from alias2k/fix/designer-stable-node-ids\n\nfix(design): key canvas collapse + layout on stable node ids",
          "timestamp": "2026-11-10T11:19:11+01:00",
          "tree_id": "6dc2c307b9f6ef468acc6c9dbcda72d6ccb92ba9",
          "url": "https://github.com/alias2k/flusso/commit/c12abf0b799970a6e4195814b787bd1bf18274ed"
        },
        "date": 1794306987376,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 858.2770545762708,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 635.6515457942542,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 572.1543087440065,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 583.1157509541151,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "d8ad9059e41851b4624dadf9b18336f6676b2282",
          "message": "Merge pull request #151 from alias2k/fix/partitioned-tables\n\nfix(source-postgres): stream partitioned tables via the partition root",
          "timestamp": "2026-11-10T11:25:15+01:00",
          "tree_id": "2fa8ee8c968e8bd232d99eac76be4d3201cd1246",
          "url": "https://github.com/alias2k/flusso/commit/d8ad9059e41851b4624dadf9b18336f6676b2282"
        },
        "date": 1794308054683,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 548.5031861996099,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 461.8225480686458,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 423.4607795846469,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 426.9562064861681,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "82f904a78e009604fbc18cf6b9d72c249b5bcad3",
          "message": "Merge pull request #159 from alias2k/feat/query-page-cursor\n\nfeat(query): page cursors — PIT + search_after alongside from/size",
          "timestamp": "2026-11-10T14:45:38+01:00",
          "tree_id": "c8e000c80e911709c4268119ae070db0f3325de2",
          "url": "https://github.com/alias2k/flusso/commit/82f904a78e009604fbc18cf6b9d72c249b5bcad3"
        },
        "date": 1794319217785,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 992.6725843406716,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 762.7370054302683,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 818.7838626926508,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 804.9188738570169,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "5be4efaa0dbd9a5cb04d60e30c0e69da16ae8596",
          "message": "Merge pull request #178 from alias2k/feat/query-walk-stream\n\nfeat(query): stream a cursor walk — Walk<I> over pages, hits, and ids",
          "timestamp": "2026-11-10T15:30:50+01:00",
          "tree_id": "6b2f542450b88731fe9f5c4eef2c853d620cfd10",
          "url": "https://github.com/alias2k/flusso/commit/5be4efaa0dbd9a5cb04d60e30c0e69da16ae8596"
        },
        "date": 1794321916664,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 1150.3609486398186,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 866.4657991798532,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 878.3293244992275,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 900.3484006000618,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "b0fe062b1c21b0088e206816de95962fdb82a76d",
          "message": "Merge pull request #176 from alias2k/feat/opensearch-index-settings\n\nfeat(sink): OpenSearch index settings, sink-wide and per index",
          "timestamp": "2026-11-10T15:32:39+01:00",
          "tree_id": "ab73172d9d4256f855ef2bffb91e6feb7b8b8935",
          "url": "https://github.com/alias2k/flusso/commit/b0fe062b1c21b0088e206816de95962fdb82a76d"
        },
        "date": 1794322979821,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 759.7340930674264,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 423.09224735586304,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 425.4776602134431,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 428.1724306243539,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "077bdc84e2fd8d641cc5ee25279713f615871c20",
          "message": "Merge pull request #179 from alias2k/docs/index-settings-how-to\n\ndocs: lead the how-to guides to OpenSearch index settings",
          "timestamp": "2026-11-10T16:05:03+01:00",
          "tree_id": "89c7f4b7d5c69aa76ba1b321b55e8b062a31889e",
          "url": "https://github.com/alias2k/flusso/commit/077bdc84e2fd8d641cc5ee25279713f615871c20"
        },
        "date": 1794324061830,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 832.8898011212532,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 704.0322165496895,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 655.3656988719403,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 715.8974849825756,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "f231115e76f17aea33038497e657dc880d39ec76",
          "message": "Merge pull request #175 from alias2k/fix/self-referencing-has-many\n\nfix(source-postgres): rebuild the parent document on a self-referencing join",
          "timestamp": "2026-11-10T16:13:15+01:00",
          "tree_id": "25e74a556f514ff24821755d3ee988596c236971",
          "url": "https://github.com/alias2k/flusso/commit/f231115e76f17aea33038497e657dc880d39ec76"
        },
        "date": 1794325056849,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 854.7225803608364,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 641.6888442377596,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 582.9138057716751,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 679.6646807818263,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "103432a041b38dcaac40ac19f2b020addd2e02f1",
          "message": "Merge pull request #180 from alias2k/fix/map-keys-like-plain-fields\n\nfix!: map keys are mapped and queried like a plain field of their values kind",
          "timestamp": "2026-11-10T16:55:07+01:00",
          "tree_id": "c9147ced235a7ee05baa2fdc41daca19e3f6c6e1",
          "url": "https://github.com/alias2k/flusso/commit/103432a041b38dcaac40ac19f2b020addd2e02f1"
        },
        "date": 1794327177813,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 735.6312693577231,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 481.28832399923425,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 447.4990322460632,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 462.04529623880677,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "d6d865f84f945b353c6c4768517dd708f0ff415f",
          "message": "Merge pull request #187 from alias2k/feat/stored-not-mapped\n\nfeat(schema): keep content without indexing it (subfields: false, indexed: false)",
          "timestamp": "2026-11-10T23:58:29+01:00",
          "tree_id": "d3af2b9a8e85e3b82762c89813175b7c9eb9a5fc",
          "url": "https://github.com/alias2k/flusso/commit/d6d865f84f945b353c6c4768517dd708f0ff415f"
        },
        "date": 1794352508439,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 757.8122489628516,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 589.4077700954049,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 608.7421360922561,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 664.6576731997293,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "473b0ad1c610d7d49172068b279064affd3a2bd6",
          "message": "Merge pull request #189 from alias2k/feat/map-mirror\n\nfeat(query)!: a map handle mirrors its values kind across all keys",
          "timestamp": "2026-11-11T00:28:45+01:00",
          "tree_id": "e55e33bca2ac6d43981e91064c78e3ec1f84f18a",
          "url": "https://github.com/alias2k/flusso/commit/473b0ad1c610d7d49172068b279064affd3a2bd6"
        },
        "date": 1794354266357,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 832.5985716852265,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 628.0262828070166,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 583.9468678455531,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 608.2542538481013,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "9bf0e338465b652952133adb5bd0b95de974298e",
          "message": "Merge pull request #188 from alias2k/feat/skip-unprojected-rebuild\n\nfeat(source): skip the rebuild when the changed columns are not used by the path",
          "timestamp": "2026-11-11T00:29:31+01:00",
          "tree_id": "7984e68e4a8ea3d8c7bcd71b3cecfd5ba7cff06f",
          "url": "https://github.com/alias2k/flusso/commit/9bf0e338465b652952133adb5bd0b95de974298e"
        },
        "date": 1794355309677,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 726.6021192265599,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 499.43527725148255,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 418.79503319633994,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 650.8218153636656,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "817f78590884bb3f36679be7bb930ac57b0397f3",
          "message": "Merge pull request #185 from alias2k/feat/read-your-writes\n\nfeat: a client can wait until its commit is readable (read-your-writes fence)",
          "timestamp": "2026-11-11T10:04:19+01:00",
          "tree_id": "77e0a2e2b25074a22ebffa4c249913847bed03a4",
          "url": "https://github.com/alias2k/flusso/commit/817f78590884bb3f36679be7bb930ac57b0397f3"
        },
        "date": 1794388745360,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 1074.3770894186825,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 969.0245137044407,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 833.2128132211685,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 1108.999329000598,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "9467483d56ec78e69f18c470c927c9f607b39477",
          "message": "Merge pull request #193 from alias2k/feat/keyword-map-exact\n\nfeat(query): exact operators on a keyword map with subfields",
          "timestamp": "2026-11-11T10:20:25+01:00",
          "tree_id": "74f055c2e03f493ae4b32ce3a8716fe2317ac8cf",
          "url": "https://github.com/alias2k/flusso/commit/9467483d56ec78e69f18c470c927c9f607b39477"
        },
        "date": 1794389881510,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 691.7617749870005,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 499.21373921197124,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 412.8785460349273,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 599.7916782235741,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0bea26b48112de0e5cfb924939d0e142d00938fe",
          "message": "Merge pull request #192 from alias2k/feat/aggregates-across-hops\n\nfeat(schema): aggregates and exists across several hops",
          "timestamp": "2026-11-11T11:06:44+01:00",
          "tree_id": "3d61f9d3a547a0685cafc16ce098d4a9981c7f05",
          "url": "https://github.com/alias2k/flusso/commit/0bea26b48112de0e5cfb924939d0e142d00938fe"
        },
        "date": 1794392637818,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 719.8585763249461,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 488.33623231259554,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 414.774799458357,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 631.3562384785355,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "da68a54c573dbf9e4d674edc7a14ee9ea3426b59",
          "message": "Merge pull request #196 from alias2k/feat/join-on-columns\n\nfeat(schema): on: column pairs on joins and aggregates",
          "timestamp": "2026-11-11T12:02:25+01:00",
          "tree_id": "10f7d152386f74226cf8e614193ba0f45f98c057",
          "url": "https://github.com/alias2k/flusso/commit/da68a54c573dbf9e4d674edc7a14ee9ea3426b59"
        },
        "date": 1794396004719,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 777.6993667575907,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 542.6426927954423,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 450.07654718109,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 662.0288410046977,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "7be25032055b45a8ecc598c9312941bf5ff7c5c6",
          "message": "Merge pull request #200 from alias2k/feat/aggregate-expressions\n\nfeat(schema): arithmetic expressions as an aggregate's input",
          "timestamp": "2026-11-11T13:23:15+01:00",
          "tree_id": "c0cefa440858946ba4172b0aedfc85757bf62791",
          "url": "https://github.com/alias2k/flusso/commit/7be25032055b45a8ecc598c9312941bf5ff7c5c6"
        },
        "date": 1794400825917,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 753.4469649572318,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 674.4443793004461,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 578.6344370741593,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 861.8883228378758,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "51d2566e4b8aebcb8908595f03ebf026d213722d",
          "message": "Merge pull request #203 from alias2k/fix/pre-0-17-hardening\n\nfix: pre-0.17 hardening — NULLIF on division, check verifies relations, REPLICA IDENTITY FULL by default",
          "timestamp": "2026-11-11T15:21:08+01:00",
          "tree_id": "90f4abff8e7dca69962464f7589c3057a81c84ff",
          "url": "https://github.com/alias2k/flusso/commit/51d2566e4b8aebcb8908595f03ebf026d213722d"
        },
        "date": 1794407820865,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 770.7273667422085,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 701.8921627151047,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 564.9113742365871,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 878.8823773437141,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0d40580a10234c435620a6fe7731e4a2c63134fc",
          "message": "Merge pull request #199 from alias2k/feat/script-fields\n\nfeat(schema): script fields — typed computed fields in Rhai",
          "timestamp": "2026-11-11T15:57:14+01:00",
          "tree_id": "62f3b61c6edb2611e720b9f89063593da184ba72",
          "url": "https://github.com/alias2k/flusso/commit/0d40580a10234c435620a6fe7731e4a2c63134fc"
        },
        "date": 1794410175839,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 650.2502422707229,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 514.7865262213516,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 410.6192525236034,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 580.1623738169696,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "jakub.wasilczyk@alias2k.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "jakub.wasilczyk@alias2k.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "distinct": true,
          "id": "9bc62c1759b20639f16e498232b58b2cf0f89aa0",
          "message": "ci(pages): name the artifact per attempt so a re-run can deploy\n\nA re-run keeps the earlier attempt's github-pages artifact, and\ndeploy-pages refuses a name that matches two.",
          "timestamp": "2026-11-11T16:04:52+01:00",
          "tree_id": "223c1eb92efb981250a5a957fc88963401722555",
          "url": "https://github.com/alias2k/flusso/commit/9bc62c1759b20639f16e498232b58b2cf0f89aa0"
        },
        "date": 1794411291623,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 635.2791737807333,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 521.4134103117088,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 425.42328734646424,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 645.1685231510047,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "2d48e9936e1c7ab1ceb0a830440ea37f5b110adb",
          "message": "Merge pull request #207 from alias2k/test/wal-release-guard\n\ntest: guard that the replication slot releases WAL flusso has no use for",
          "timestamp": "2026-11-11T17:07:49+01:00",
          "tree_id": "07cb74001b442d2ccd553403accedca92f076fb8",
          "url": "https://github.com/alias2k/flusso/commit/2d48e9936e1c7ab1ceb0a830440ea37f5b110adb"
        },
        "date": 1794414212232,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 1132.2281377085267,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 1016.3634599708153,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 856.3435559019172,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 1222.7371446275847,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "004fe08c2e26dbd30175e7ec694dc1e4c1536dca",
          "message": "Merge pull request #208 from alias2k/fix/fast-wal-release\n\nfix(source-postgres): release WAL within seconds of the last sink flush",
          "timestamp": "2026-11-11T17:44:14+01:00",
          "tree_id": "5eed7cd77f64f6cfe5bd3ccfb5123705af8a3f58",
          "url": "https://github.com/alias2k/flusso/commit/004fe08c2e26dbd30175e7ec694dc1e4c1536dca"
        },
        "date": 1794416460395,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 882.2604025695194,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 750.4680779353924,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 583.0684192362191,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 901.0826829088479,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "b28e88b8d5282705bfca07344972961b50ca2f6e",
          "message": "Merge pull request #210 from alias2k/ci/manual-from-release\n\nci(pages): serve the newest release's manual at the root and main's under /main/",
          "timestamp": "2026-11-11T20:53:00+01:00",
          "tree_id": "8b2910ecb56ac0302507954ccf7059e51537eaae",
          "url": "https://github.com/alias2k/flusso/commit/b28e88b8d5282705bfca07344972961b50ca2f6e"
        },
        "date": 1794427904394,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 661.630197338921,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 488.3093541755407,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 409.5540533774447,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 634.6214533146225,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "84f46ae21052633aeae9fce8def61fb44a560c3f",
          "message": "Merge pull request #220 from alias2k/chore/feature-round-leftovers\n\nchore: feature-round leftovers (map hash pin, upgrade note, subfields/scope UI tests)",
          "timestamp": "2026-11-11T23:23:58+01:00",
          "tree_id": "a6d06290083a1bc6cf1846083d5aecb86a43c9a8",
          "url": "https://github.com/alias2k/flusso/commit/84f46ae21052633aeae9fce8def61fb44a560c3f"
        },
        "date": 1794436938288,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 699.6342430765201,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 509.0736963103199,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 413.9044765167505,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 626.4973885924353,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "b794184c822a155f72d93943d3cdc9084b1a4a72",
          "message": "Merge pull request #223 from alias2k/fix/respect-using-index-identity\n\nfix(source-postgres): keep a USING INDEX replica identity that carries every link",
          "timestamp": "2026-11-11T23:42:05+01:00",
          "tree_id": "edf54309ebefe5bae4b58af72ed9183d5d5af0df",
          "url": "https://github.com/alias2k/flusso/commit/b794184c822a155f72d93943d3cdc9084b1a4a72"
        },
        "date": 1794438026742,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 814.3373162762842,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 687.3033461752316,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 551.2086376655825,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 811.1946665821295,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "7319433b9ca009e2670337a5d2e1e385e5c3472b",
          "message": "Merge pull request #222 from alias2k/fix/validate-soft-delete-and-run\n\nfix(check): verify the soft-delete marker; run refuses to start on what cannot work as written",
          "timestamp": "2026-11-11T23:46:09+01:00",
          "tree_id": "78660f8ae156ee81a75013ec3387b8e4df9fa284",
          "url": "https://github.com/alias2k/flusso/commit/7319433b9ca009e2670337a5d2e1e385e5c3472b"
        },
        "date": 1794439592295,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 675.4715278701675,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 506.9197750117803,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 415.628802687298,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 606.223154054433,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "7fb80e21f089d79aeffe8fc9dae646420b338b3c",
          "message": "Merge pull request #235 from alias2k/chore/refresh-lockfile\n\nchore(deps): refresh Cargo.lock, take nutype 0.8 and base64 0.23",
          "timestamp": "2026-11-12T00:06:26+01:00",
          "tree_id": "131bb2029e84d76a3553982c1a8046b5c8595e69",
          "url": "https://github.com/alias2k/flusso/commit/7fb80e21f089d79aeffe8fc9dae646420b338b3c"
        },
        "date": 1794441192671,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 687.1431554577142,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 503.31335545618316,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 410.4572688279918,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 584.7229832092385,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "9d78c7f34fb5cefc18c6f92345ede3f4d99bc807",
          "message": "Merge pull request #236 from alias2k/chore/pgwire-replication-0-5\n\nchore(source-postgres): upgrade pgwire-replication to 0.5",
          "timestamp": "2026-11-12T00:32:49+01:00",
          "tree_id": "f2782a773d4214820ad0fb2088d7a204215f659e",
          "url": "https://github.com/alias2k/flusso/commit/9d78c7f34fb5cefc18c6f92345ede3f4d99bc807"
        },
        "date": 1794442250416,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 958.2873597168584,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 738.967011986856,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 585.0529078620123,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 874.1160114208526,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "3dcc7776db559ebe6de2c61f8a35a9dcffa43cdc",
          "message": "Merge pull request #238 from alias2k/chore/otel-0-33-reqwest-0-13\n\nchore(deps): OpenTelemetry 0.33 and reqwest 0.13 together, one reqwest",
          "timestamp": "2026-11-12T00:55:03+01:00",
          "tree_id": "a7ca513c499635163af5d786a9a5a287f4a71019",
          "url": "https://github.com/alias2k/flusso/commit/3dcc7776db559ebe6de2c61f8a35a9dcffa43cdc"
        },
        "date": 1794443412219,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 726.0049823885148,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 421.6568566561893,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 420.1176061199763,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 614.0778570158011,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "b82de056d3e1d479b161317ec533fbb55a33075b",
          "message": "Merge pull request #241 from alias2k/test/opensearch-https-e2e\n\nfix(sink-opensearch): report transport errors with their cause; pin the sink and flusso-query over https",
          "timestamp": "2026-11-12T11:45:49+01:00",
          "tree_id": "b50f681dccdfa1a5fb51d05e5bf8b8dfa11ffc01",
          "url": "https://github.com/alias2k/flusso/commit/b82de056d3e1d479b161317ec533fbb55a33075b"
        },
        "date": 1794481450433,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 649.5117742425988,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 510.6183488840692,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 416.350218608561,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 621.9708208338027,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "43ef9c18e1aa209e408efa9ff0bdfb1e8b591140",
          "message": "Merge pull request #240 from alias2k/fix/partition-identity-without-pk\n\nfix(source-postgres): set a partition whose replica identity leaves out the primary key",
          "timestamp": "2026-11-12T12:12:04+01:00",
          "tree_id": "6b43eeb1527b10566558bda7fe316844f2c98b8e",
          "url": "https://github.com/alias2k/flusso/commit/43ef9c18e1aa209e408efa9ff0bdfb1e8b591140"
        },
        "date": 1794483061176,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 609.7998925340502,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 477.74637981028974,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 407.6697250553942,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 592.8694674227049,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "bf716b92fe8b19a009bdf37b148fdaae81da0558",
          "message": "Merge pull request #153 from alias2k/release-plz-2026-11-10T10-20-59Z\n\nchore: release v0.17.0",
          "timestamp": "2026-11-12T13:56:14+01:00",
          "tree_id": "f677450a7eb76a91a69790173285b2d63f1f547d",
          "url": "https://github.com/alias2k/flusso/commit/bf716b92fe8b19a009bdf37b148fdaae81da0558"
        },
        "date": 1794489130843,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 1141.6539602721489,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 1010.6812409941792,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 803.7359127000196,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 1181.664541059588,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "205f155a7df72c6a97d74b5d1a8c841bc150d1e9",
          "message": "Merge pull request #227 from alias2k/dependabot/github_actions/github-actions-353a688cfa\n\nchore(deps): bump the github-actions group across 1 directory with 12 updates",
          "timestamp": "2026-11-12T14:11:21+01:00",
          "tree_id": "7d99bbb0fc10d7c659a3cdb670d7b999f543eece",
          "url": "https://github.com/alias2k/flusso/commit/205f155a7df72c6a97d74b5d1a8c841bc150d1e9"
        },
        "date": 1794490278075,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 672.9903686208238,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 508.4651021557036,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 416.31816303924154,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 590.8218368497144,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "5ca534e576aaac8326ce74c4ed822eaef2e5c9a8",
          "message": "Merge pull request #245 from alias2k/chore/designer-deps\n\nchore(design): land the designer frontend dependency updates (#228, #231)",
          "timestamp": "2026-11-12T14:22:17+01:00",
          "tree_id": "9091df7425a726d6b59f971331971a8a0be177f7",
          "url": "https://github.com/alias2k/flusso/commit/5ca534e576aaac8326ce74c4ed822eaef2e5c9a8"
        },
        "date": 1794491256223,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 1251.7558003084184,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 999.9284851776946,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 827.5223113196762,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 1130.9620085263143,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "cebe4b190545119a0331c0d87ed22e5c953a8d3b",
          "message": "Merge pull request #246 from alias2k/fix/dead-scope-attribute\n\nfix(query-derive)!: a scope attribute that renames nothing is a compile error",
          "timestamp": "2026-11-12T14:42:39+01:00",
          "tree_id": "31af0ddd1c17c5a663e73f731ea77404cf86ab74",
          "url": "https://github.com/alias2k/flusso/commit/cebe4b190545119a0331c0d87ed22e5c953a8d3b"
        },
        "date": 1794492391417,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 624.9662654928005,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 476.5824100346152,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 410.66508077527135,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 596.5079493143977,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "59db780031f379c84b86a7b62620c824bd675e84",
          "message": "Merge pull request #250 from alias2k/feat/private-ca\n\nfeat: trust a private CA (ca_file) in the sink, query client and admin client; never retry a refused certificate",
          "timestamp": "2026-11-12T15:04:51+01:00",
          "tree_id": "37c779f2ae5fdd7160c2d2f4c5c3e3a4b9a04e23",
          "url": "https://github.com/alias2k/flusso/commit/59db780031f379c84b86a7b62620c824bd675e84"
        },
        "date": 1794493517700,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 761.4365416671284,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 474.5562303747641,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 415.52688491618557,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 621.3374162075362,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "390406315f6cada0d85bf30ec4c1cef9541b025f",
          "message": "Merge pull request #249 from alias2k/fix/check-validation-gaps\n\nfix!: check reports schemas that cannot work; run refuses the ones no deployment survives",
          "timestamp": "2026-11-12T15:25:11+01:00",
          "tree_id": "e864bc57fa448aa9d47acfefd1356a913866a03f",
          "url": "https://github.com/alias2k/flusso/commit/390406315f6cada0d85bf30ec4c1cef9541b025f"
        },
        "date": 1794494560511,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 830.8289583344723,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 761.7706986358365,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 580.0731553698678,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 866.9127591338828,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "2661fa55c5f78756ecb45cd19320177ecef9cfac",
          "message": "Merge pull request #254 from alias2k/chore/rand-0-10\n\nchore(deps): bump rand to 0.10 in the bench harness",
          "timestamp": "2026-11-12T16:54:32+01:00",
          "tree_id": "c97d7667e02a0c182fcb85b15c46888cf9981e2f",
          "url": "https://github.com/alias2k/flusso/commit/2661fa55c5f78756ecb45cd19320177ecef9cfac"
        },
        "date": 1794500461478,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 649.7773110652608,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 497.10186450683955,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 416.27877570314934,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 611.5056557409329,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "3fda53dd3d3ca0f553a5a22ab7a80e984a5ee796",
          "message": "Merge pull request #255 from alias2k/fix/ignored-derive-attributes\n\nfix(query-derive)!: reject opaque, rename and scope_mod where they do nothing",
          "timestamp": "2026-11-12T17:12:45+01:00",
          "tree_id": "7eb11846484d65f2d6574dffcde9aaf1a8484bd9",
          "url": "https://github.com/alias2k/flusso/commit/3fda53dd3d3ca0f553a5a22ab7a80e984a5ee796"
        },
        "date": 1794501496721,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 998.6842120788755,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 829.3404660088576,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 637.9678902535989,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 975.4148786837737,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "1d86376808a7fdf3a0725f78f238bd9c7a37bf3c",
          "message": "Merge pull request #253 from alias2k/fix/designer-blank-name-preview\n\nfix(design): write values as plain YAML so Save stops turning numbers into strings; keep incomplete fields out of the preview",
          "timestamp": "2026-11-12T17:53:25+01:00",
          "tree_id": "0e917d6d2fc6ed3c0c7a259ed9df6bfdaee79143",
          "url": "https://github.com/alias2k/flusso/commit/1d86376808a7fdf3a0725f78f238bd9c7a37bf3c"
        },
        "date": 1794503426739,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 819.7037616344152,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 682.264980202852,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 561.4177187671978,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 876.6310476744253,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "5100d54b1bfc443571598861350200622b17e73e",
          "message": "Merge pull request #260 from alias2k/docs/testing-e2e-table\n\ndocs(contribute): list every e2e suite on the testing page",
          "timestamp": "2026-11-12T21:45:51+01:00",
          "tree_id": "bcbd009e1638f66a5e911c5501ab9aa8db6a5a6d",
          "url": "https://github.com/alias2k/flusso/commit/5100d54b1bfc443571598861350200622b17e73e"
        },
        "date": 1794517880828,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 719.3742809703442,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 697.2763186879266,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 565.5669024423424,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 774.1981404577997,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "8c75ab5633db8d51c0c9324e7b7cb13b36605916",
          "message": "Merge pull request #261 from alias2k/fix/derive-attribute-gaps\n\nfix(query-derive)!: reject the remaining unread attributes",
          "timestamp": "2026-11-12T22:28:48+01:00",
          "tree_id": "15b3003e85e4fc95702062740df3269ba52821a4",
          "url": "https://github.com/alias2k/flusso/commit/8c75ab5633db8d51c0c9324e7b7cb13b36605916"
        },
        "date": 1794519937917,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 852.2925465852995,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 722.7974043104397,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 567.0819613976835,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 843.4820421079257,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "05088cf3199a20471505ce74cdde5d034873a055",
          "message": "Merge pull request #263 from alias2k/fix/derive-root-fragment-parity\n\nfix(query-derive)!: root and fragment agree on maps; reject a rename serde does not see",
          "timestamp": "2026-11-13T00:28:58+01:00",
          "tree_id": "c999a5347674e6c81d4a51db2666f24d7f3c479d",
          "url": "https://github.com/alias2k/flusso/commit/05088cf3199a20471505ce74cdde5d034873a055"
        },
        "date": 1794527236130,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 623.7696763006087,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 531.705396322297,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 407.8584108737087,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 608.545778852377,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "1515eafa253e9564e0583a9ccddd1bca5664fe1b",
          "message": "Merge pull request #264 from alias2k/fix/designer-number-values\n\nfix(design): a number typed in the inspector is saved as the value it previews",
          "timestamp": "2026-11-13T03:08:08+01:00",
          "tree_id": "b4ecd9b2063df8914587566902407ba5bb73e4ab",
          "url": "https://github.com/alias2k/flusso/commit/1515eafa253e9564e0583a9ccddd1bca5664fe1b"
        },
        "date": 1794536735551,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 852.0539324100447,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 736.3553161438717,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 572.1080428802816,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 903.7091557761665,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "42f47438aeb6c41f0e3853309f8c3903ae33c666",
          "message": "Merge pull request #271 from alias2k/fix/design-focus-selection-267\n\nfix(design): keep focus in an option's key box; select a field by its uid",
          "timestamp": "2026-11-13T10:17:48+01:00",
          "tree_id": "f18e436b4c1aac728ba67111f65e842cfbe074eb",
          "url": "https://github.com/alias2k/flusso/commit/42f47438aeb6c41f0e3853309f8c3903ae33c666"
        },
        "date": 1794562591014,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 676.9310303211383,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 494.81052161635205,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 415.9725344836719,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 597.9901825649082,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "19d16eaf8ed980f9e91ffe2239608aeb5abb0da6",
          "message": "Merge pull request #268 from alias2k/fix/derive-nested-maps-265\n\nfix(query-derive)!: arrays of maps, newtypes over a Vec, hand-written values in fragments",
          "timestamp": "2026-11-13T11:34:46+01:00",
          "tree_id": "b5ade201222ddfb79eafe4a232b17890eb9c21cd",
          "url": "https://github.com/alias2k/flusso/commit/19d16eaf8ed980f9e91ffe2239608aeb5abb0da6"
        },
        "date": 1794567123085,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 583.0429539888988,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 764.27632298262,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 566.8200541224849,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 713.9337570053076,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "1c61e573592ee35171d63e2c4adb01a2354f7411",
          "message": "Merge pull request #269 from alias2k/feat/design-keep-comments-257\n\nfeat(design): a save keeps the file's comments and untouched layout",
          "timestamp": "2026-11-13T11:46:57+01:00",
          "tree_id": "9219adca395270ab317fa34af6bcd81912b7674c",
          "url": "https://github.com/alias2k/flusso/commit/1c61e573592ee35171d63e2c4adb01a2354f7411"
        },
        "date": 1794568225599,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 617.9366269166254,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 484.80684495213194,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 418.6119229468153,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 606.7941636465123,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0c333c4012ff7e84d28967852581a8ef76a60b81",
          "message": "Merge pull request #272 from alias2k/fix/design-index-removal-270\n\nfix(design): pair index rows by id, and leave a removed index's file on disk",
          "timestamp": "2026-11-13T13:26:05+01:00",
          "tree_id": "6111850dabdc813a46f7b4715ff1c5239591df26",
          "url": "https://github.com/alias2k/flusso/commit/0c333c4012ff7e84d28967852581a8ef76a60b81"
        },
        "date": 1794573875204,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 748.9758127439973,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 604.4496450257354,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 538.012380223673,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 807.6567183081644,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "abfb4004340e7b948144312635d62007060ef9f3",
          "message": "Merge pull request #278 from alias2k/fix/design-toml-line-endings-273\n\nfix(design): keep a flusso.toml's line endings and match sink keys as read",
          "timestamp": "2026-11-13T15:08:29+01:00",
          "tree_id": "4187a65d669b88cd0ceb8c54ea44de30d998fbf6",
          "url": "https://github.com/alias2k/flusso/commit/abfb4004340e7b948144312635d62007060ef9f3"
        },
        "date": 1794580030557,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 649.1000623748486,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 567.3267117078832,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 437.0981992490018,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 710.8568855560812,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "22596b0b605fc4cada91ea24a5b4c4a421cbf911",
          "message": "Merge pull request #283 from alias2k/fix/design-sink-name-282\n\nfix(design): a sink name is applied as flusso reads it",
          "timestamp": "2026-11-13T19:03:39+01:00",
          "tree_id": "d32414b1f2069623dffa9639b87c2bf726ef30e5",
          "url": "https://github.com/alias2k/flusso/commit/22596b0b605fc4cada91ea24a5b4c4a421cbf911"
        },
        "date": 1794594624195,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 648.7546245751673,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 480.0847886762614,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 408.44472347337233,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 608.2901037936454,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "2fb54e33104f8dc9f96c2bc20f32ded71b27531a",
          "message": "Merge pull request #277 from alias2k/fix/design-save-recovery-274\n\nfix(design): recover a save the designer was killed in the middle of",
          "timestamp": "2026-11-13T19:40:07+01:00",
          "tree_id": "aff3a68c0187cd98fd7d5429c99b6361bfc7a29d",
          "url": "https://github.com/alias2k/flusso/commit/2fb54e33104f8dc9f96c2bc20f32ded71b27531a"
        },
        "date": 1794596242612,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 881.8653588570471,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 669.6351132164259,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 551.3175085867914,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 815.8365581673617,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "1d101521f9670828ce1cfa2362e0ad22a2a1aacc",
          "message": "Merge pull request #281 from alias2k/feat/array-columns\n\nfeat(schema): declare a column field as an array (array: true)",
          "timestamp": "2026-11-13T21:11:00+01:00",
          "tree_id": "04764e57b87ad4538ba7bfc331554a3d6a0b5f80",
          "url": "https://github.com/alias2k/flusso/commit/1d101521f9670828ce1cfa2362e0ad22a2a1aacc"
        },
        "date": 1794601723192,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 870.4028207847499,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 774.6154614992048,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 556.691907590931,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 894.3994124760063,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "d064c5f76278255952cff2232d3a9993457e9c25",
          "message": "Merge pull request #288 from alias2k/fix/design-toml-layout-279\n\nfix(design): keep the layout #279 lists through a save",
          "timestamp": "2026-11-13T21:42:39+01:00",
          "tree_id": "ba6117943bcdd290f7e53dd66c2d87c7c05d901d",
          "url": "https://github.com/alias2k/flusso/commit/d064c5f76278255952cff2232d3a9993457e9c25"
        },
        "date": 1794603505156,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 1087.593207669149,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 977.9557566494383,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 812.0677487279341,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 1071.3284436870292,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "c46da9cdf444af4348d3d9d7be105f2f60a34453",
          "message": "Merge pull request #286 from alias2k/fix/design-stale-page-280\n\nfix(design): refuse a save over a file changed since the page loaded it",
          "timestamp": "2026-11-13T23:17:51+01:00",
          "tree_id": "781fac72a3259b614cd3b4de4910bc40ee209176",
          "url": "https://github.com/alias2k/flusso/commit/c46da9cdf444af4348d3d9d7be105f2f60a34453"
        },
        "date": 1794609395751,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 615.0583140800835,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 509.079985204704,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 416.3293520873917,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 619.88511416805,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "2e276252b15fdebe83cc93c2da7f64d8108d73f5",
          "message": "Merge pull request #290 from alias2k/fix/derive-reads-array-289\n\nfix(query-derive)!: a newtype reads an array only as one of the two array kinds",
          "timestamp": "2026-11-14T01:41:11+01:00",
          "tree_id": "d5f455460fbfce0801193b9978b51c576fbd239c",
          "url": "https://github.com/alias2k/flusso/commit/2e276252b15fdebe83cc93c2da7f64d8108d73f5"
        },
        "date": 1794617806961,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 1094.2320049238253,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 998.4437117475214,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 815.460877457466,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 1182.1231263123439,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "c29368269e593c61f21de471dacf1f66202f8e68",
          "message": "Merge pull request #293 from alias2k/fix/binary-and-script-run-291\n\nfix: binary columns index their bytes; constants build as written; script run shows a failed build",
          "timestamp": "2026-11-14T02:48:15+01:00",
          "tree_id": "92835a4ea784989a3e7410b12a108bf825c3e185",
          "url": "https://github.com/alias2k/flusso/commit/c29368269e593c61f21de471dacf1f66202f8e68"
        },
        "date": 1794621966420,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 728.1790021769177,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 729.3320149668649,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 571.4386815007929,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 876.8438332510055,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "056b4292c1aef1ad7a75f64baaf7ad7fa86e2b1c",
          "message": "Merge pull request #296 from alias2k/fix/design-recovery-notice-292\n\nfix(design): keep the recovery notice until it is dismissed, and say what happened to each file",
          "timestamp": "2026-11-14T04:35:38+01:00",
          "tree_id": "6d7ec54a7217faaff5ce8b8f6c3c8ccd49317275",
          "url": "https://github.com/alias2k/flusso/commit/056b4292c1aef1ad7a75f64baaf7ad7fa86e2b1c"
        },
        "date": 1794628391752,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 937.1575517946901,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 648.2260716149515,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 567.48535530414,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 835.5473878853494,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "464af3c04085fe6dc8f39b9e8c70fc1f608720b6",
          "message": "Merge pull request #303 from alias2k/fix/sink-long-document-id-301\n\nfix(sink-opensearch): reject an id OpenSearch refuses instead of failing the bulk",
          "timestamp": "2026-11-14T05:26:10+01:00",
          "tree_id": "919496ef1a8f7c35213b6c7c38c2877fcec9165b",
          "url": "https://github.com/alias2k/flusso/commit/464af3c04085fe6dc8f39b9e8c70fc1f608720b6"
        },
        "date": 1794631374904,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 857.4899518030495,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 667.1054902448294,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 583.388517886523,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 900.902380811888,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "fb3f94b28d42c0993e1d83e3ff180cc6342b0def",
          "message": "Merge pull request #287 from alias2k/fix/design-duplicate-names-284\n\nfix(design): refuse a field name its level already has; warn about duplicates; read typed table/column names as flusso does",
          "timestamp": "2026-11-14T07:09:40+01:00",
          "tree_id": "01a8c0ff4b3a380038e45dd5e0e39ee242377d29",
          "url": "https://github.com/alias2k/flusso/commit/fb3f94b28d42c0993e1d83e3ff180cc6342b0def"
        },
        "date": 1794638146447,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 641.8831872944724,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 534.0941268226977,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 412.41987638832074,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 607.5376232220401,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0cde709f0e51bdf6a0d4b6454dc8b9b6498eccd3",
          "message": "Merge pull request #297 from alias2k/fix/bytea-relation-key-295\n\nfix(source-postgres): read every key type the same way in backfill, WAL and build",
          "timestamp": "2026-11-14T08:48:38+01:00",
          "tree_id": "595925eb34606b786ef44451aa82d686758d63e5",
          "url": "https://github.com/alias2k/flusso/commit/0cde709f0e51bdf6a0d4b6454dc8b9b6498eccd3"
        },
        "date": 1794643460933,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 1072.0373134593767,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 1046.8674890879265,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 810.0843436697254,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 1110.7248551726473,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "247dc1acb78ca702ecfd2916b85d65609900ff00",
          "message": "Merge pull request #305 from alias2k/fix/design-resettle-302\n\nfix(design): a rollback settles each file once, even across kills",
          "timestamp": "2026-11-14T09:11:31+01:00",
          "tree_id": "ad182f8b91a9f7cea383c385c24764b4f5f0c303",
          "url": "https://github.com/alias2k/flusso/commit/247dc1acb78ca702ecfd2916b85d65609900ff00"
        },
        "date": 1794645026605,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 630.647139268285,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 457.43204904420446,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 407.6415634433954,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 570.4100808203781,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "28298fff5346af8ccbb690534e21f60dbffead8c",
          "message": "Merge pull request #318 from alias2k/ci/bench-regression-spread-317\n\nci(bench): a noisy series needs a move larger than its own swings",
          "timestamp": "2026-11-14T09:30:16+01:00",
          "tree_id": "0a52270f4dad33609f5703e58afff739004210ac",
          "url": "https://github.com/alias2k/flusso/commit/28298fff5346af8ccbb690534e21f60dbffead8c"
        },
        "date": 1794646048494,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 845.6562038233394,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 680.8483342954233,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 561.8231325530123,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 865.1941190604101,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "4ed604e98c9cbab39cf9f5240c94ead5cccf474f",
          "message": "Merge pull request #314 from alias2k/test/golden-fixture-duplicate-name\n\ntest(config): the golden fixture names each root field once",
          "timestamp": "2026-11-14T09:37:16+01:00",
          "tree_id": "8bf3fec7d96970492e169dc109ae183f0efbb0b9",
          "url": "https://github.com/alias2k/flusso/commit/4ed604e98c9cbab39cf9f5240c94ead5cccf474f"
        },
        "date": 1794647174726,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 646.0116191623982,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 487.67867944252714,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 400.3061989119173,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 537.1317160992412,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "f12cd8ec32b7c83135f8753e1bc6457b096f6ac1",
          "message": "Merge pull request #310 from alias2k/fix/naive-timestamp-304\n\nfix(sink)!: write a naive timestamp as ISO 8601 with a T",
          "timestamp": "2026-11-14T10:09:53+01:00",
          "tree_id": "9e91c11b79af85a7076769c197ca560b1739e1bf",
          "url": "https://github.com/alias2k/flusso/commit/f12cd8ec32b7c83135f8753e1bc6457b096f6ac1"
        },
        "date": 1794648975165,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 605.2798394548975,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 473.75897865838334,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 412.0773724239596,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 572.7825649779646,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "1588e786a689e03a3aac4191da46ea2db02f31b2",
          "message": "Merge pull request #323 from alias2k/test/duplicate-names-wildcard-delete\n\ntest(cli): the duplicate-names matrix deletes only its own pair's indexes",
          "timestamp": "2026-11-14T10:50:18+01:00",
          "tree_id": "92691c2303cb00c0657d64971235b4290fa60b88",
          "url": "https://github.com/alias2k/flusso/commit/1588e786a689e03a3aac4191da46ea2db02f31b2"
        },
        "date": 1794650922437,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 681.5218904086636,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 505.35870298098035,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 408.6877202453629,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 615.3001684658707,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "dfbd181de15862c58a5b9ef59c55a3163a67aee1",
          "message": "Merge pull request #321 from alias2k/fix/type-pairs-that-fail-at-runtime-299\n\nfix(source-postgres): a type pair check accepts never stops the engine on a value",
          "timestamp": "2026-11-14T10:52:26+01:00",
          "tree_id": "21d57d179e1496970ada6f0a39fe6f186a938007",
          "url": "https://github.com/alias2k/flusso/commit/dfbd181de15862c58a5b9ef59c55a3163a67aee1"
        },
        "date": 1794652030258,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 656.2759682658228,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 533.4091737817597,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 410.13787537071903,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 625.4307990026589,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "6313d706c3765eb0060d7e5b2873eb240aa73689",
          "message": "Merge pull request #319 from alias2k/fix/binary-field-setting-free-298\n\nfix(source-postgres): a binary field holds the column's bytes whatever bytea_output is",
          "timestamp": "2026-11-14T11:28:55+01:00",
          "tree_id": "17e4a5e46e488631b34581844107c7f60f422d3a",
          "url": "https://github.com/alias2k/flusso/commit/6313d706c3765eb0060d7e5b2873eb240aa73689"
        },
        "date": 1794653118070,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 1036.9637648701628,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 924.16976587232,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 784.8034689454381,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 1174.2372441568787,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "5f1bb2ff127b955e4d9a113f9cceba2f7153e39e",
          "message": "Merge pull request #322 from alias2k/fix/stop-on-schemas-that-never-index-316\n\nfix(source)!: a schema that can never index a document stops the start",
          "timestamp": "2026-11-14T11:55:15+01:00",
          "tree_id": "91817103fd624e1adb47a35d320d36a006348309",
          "url": "https://github.com/alias2k/flusso/commit/5f1bb2ff127b955e4d9a113f9cceba2f7153e39e"
        },
        "date": 1794654716142,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 986.1330736368977,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 696.6525113626017,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 576.2113429313797,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 884.2834940797635,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "af3b6eab8058e73b2c4db5ff9b15cb58431e08fb",
          "message": "Merge pull request #325 from alias2k/fix/check-reports-skipped-checks-324\n\nfix(source): a check the database would not let run is reported, not dropped",
          "timestamp": "2026-11-14T12:57:35+01:00",
          "tree_id": "e7e9fa37aaccf3fad84f5d6cd272696fd2ed668c",
          "url": "https://github.com/alias2k/flusso/commit/af3b6eab8058e73b2c4db5ff9b15cb58431e08fb"
        },
        "date": 1794658427852,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 1029.5535927536139,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 787.3221689802284,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 615.103735196447,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 999.2364396748632,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "1351dcce550337fdb0c0e38600f064ec09958d2b",
          "message": "Merge pull request #326 from alias2k/fix/derive-root-fragment-agree-306\n\nfix(query-derive): a fragment accepts what a root accepts; every trusted rule is stated and pinned",
          "timestamp": "2026-11-14T14:24:37+01:00",
          "tree_id": "4b783dd02b794d1ff7cd4d53211a8657fd657d5c",
          "url": "https://github.com/alias2k/flusso/commit/1351dcce550337fdb0c0e38600f064ec09958d2b"
        },
        "date": 1794663797907,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 569.8380252052624,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 522.1006244845843,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 407.09563525694693,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 598.1776310351819,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "7a22e75366585d4d3043d1ab307dda3d8c18c1be",
          "message": "Merge pull request #329 from alias2k/chore/coordinator-skill\n\nchore(agents): add the coordinator skill and guard its paths",
          "timestamp": "2026-11-14T16:24:13+01:00",
          "tree_id": "671305778aef80d77481b0cb68dac2ccebf0bb74",
          "url": "https://github.com/alias2k/flusso/commit/7a22e75366585d4d3043d1ab307dda3d8c18c1be"
        },
        "date": 1794671033171,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 641.0451910547994,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 488.1488500189647,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 387.7726592431656,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 559.3733539001672,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "26a8161688a4ed757d57610b2d258ebb1db12df2",
          "message": "Merge pull request #330 from alias2k/docs/testing-e2e-table\n\ndocs(contribute): list every Docker-backed e2e binary and what it needs",
          "timestamp": "2026-11-14T17:58:34+01:00",
          "tree_id": "fbd174326bac28bf9fec22c3c4e4c0e680f88d18",
          "url": "https://github.com/alias2k/flusso/commit/26a8161688a4ed757d57610b2d258ebb1db12df2"
        },
        "date": 1794676640855,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 730.3848703803737,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 696.2924015454699,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 505.05309861089415,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 749.5857635933983,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "987ce77ea84f67ae6074190e1b9574229453a85f",
          "message": "Merge pull request #334 from alias2k/perf/resolve-after-297-332\n\nperf(source-postgres): stop resolve paying for root-key read-back on every change",
          "timestamp": "2026-11-14T21:35:11+01:00",
          "tree_id": "6be081f4fd14f856aa888419dd9de33ed233f2c2",
          "url": "https://github.com/alias2k/flusso/commit/987ce77ea84f67ae6074190e1b9574229453a85f"
        },
        "date": 1794690161603,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 680.6211540587104,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 482.7156581227681,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 411.5108741264896,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 593.8542356836828,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "2f95fe8400938081a1150a27a6737efd300dbc83",
          "message": "Merge pull request #333 from alias2k/fix/numeric-past-decimal-range-327\n\nfix(source-postgres)!: write every numeric digit as a JSON number",
          "timestamp": "2026-11-15T02:31:30+01:00",
          "tree_id": "3d8fe178d57559e00335bff8d5744271724ba481",
          "url": "https://github.com/alias2k/flusso/commit/2f95fe8400938081a1150a27a6737efd300dbc83"
        },
        "date": 1794707467681,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 771.6972701231834,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 657.6576758254176,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 564.8624690857517,
            "unit": "docs/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 809.2863312649279,
            "unit": "changes/s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "efe08cb0924346f78c2be65caf08afe134d134eb",
          "message": "Merge pull request #339 from alias2k/docs/coordinator-review-tooling\n\ndocs(coordinator): re-checks review the delta, and review tooling lives in the repo",
          "timestamp": "2026-11-15T19:15:20+01:00",
          "tree_id": "3812c2a0f8952715f61b8d9d82357fefdbef7aff",
          "url": "https://github.com/alias2k/flusso/commit/efe08cb0924346f78c2be65caf08afe134d134eb"
        },
        "date": 1794767586869,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 884.3730126162386,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.384, \"disk_write_ms\": 326.73, \"memory_copy_ms\": 184.434}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 709.7723813280487,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.384, \"disk_write_ms\": 326.73, \"memory_copy_ms\": 184.434}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 589.9014464406721,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.384, \"disk_write_ms\": 326.73, \"memory_copy_ms\": 184.434}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 875.301256838004,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.384, \"disk_write_ms\": 326.73, \"memory_copy_ms\": 184.434}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "f7b792daa97a2d7930026a501a8709183738f306",
          "message": "Merge pull request #336 from alias2k/fix/stale-statements-after-type-change-335\n\nfix(source-postgres): statements survive a column type change",
          "timestamp": "2026-11-15T23:36:23+01:00",
          "tree_id": "50318c297d72b17658de815e9a2f1e25b8ded3f9",
          "url": "https://github.com/alias2k/flusso/commit/f7b792daa97a2d7930026a501a8709183738f306"
        },
        "date": 1794783213335,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 1119.1543872688233,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 43.434, \"disk_write_ms\": 75.613, \"memory_copy_ms\": 201.015}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6975P-C\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 1006.3106535679968,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 43.434, \"disk_write_ms\": 75.613, \"memory_copy_ms\": 201.015}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6975P-C\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 831.9720100964181,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 43.434, \"disk_write_ms\": 75.613, \"memory_copy_ms\": 201.015}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6975P-C\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 1393.2952716382931,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 43.434, \"disk_write_ms\": 75.613, \"memory_copy_ms\": 201.015}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6975P-C\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "6bc765715e93cfce1e60ec081b27e16ce971dae7",
          "message": "Merge pull request #341 from alias2k/feat/review-ab-340\n\nfeat(bench): one command to compare a branch with its base",
          "timestamp": "2026-11-16T00:50:09+01:00",
          "tree_id": "c03e1871fc7640a6948716dbcbcf305bf50c266b",
          "url": "https://github.com/alias2k/flusso/commit/6bc765715e93cfce1e60ec081b27e16ce971dae7"
        },
        "date": 1794787785095,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 653.7067930869014,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.396, \"disk_write_ms\": 132.368, \"memory_copy_ms\": 210.254}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 507.1209701132738,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.396, \"disk_write_ms\": 132.368, \"memory_copy_ms\": 210.254}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 424.0237524203598,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.396, \"disk_write_ms\": 132.368, \"memory_copy_ms\": 210.254}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 661.9836839169732,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.396, \"disk_write_ms\": 132.368, \"memory_copy_ms\": 210.254}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "2997263701f9dfd3d27bfedddfe1eac5ac161fba",
          "message": "Merge pull request #345 from alias2k/fix/filter-sized-cast-338\n\nfix(source-postgres)!: a filter value is never cut or rounded to its column's size",
          "timestamp": "2026-11-16T01:21:53+01:00",
          "tree_id": "93ec25d69edc968beab78fb6af15f879a6ab32a4",
          "url": "https://github.com/alias2k/flusso/commit/2997263701f9dfd3d27bfedddfe1eac5ac161fba"
        },
        "date": 1794789705456,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 670.4302960821585,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.298, \"disk_write_ms\": 134.962, \"memory_copy_ms\": 213.505}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 472.72382019744236,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.298, \"disk_write_ms\": 134.962, \"memory_copy_ms\": 213.505}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 419.61914592754493,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.298, \"disk_write_ms\": 134.962, \"memory_copy_ms\": 213.505}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 644.2146194327552,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.298, \"disk_write_ms\": 134.962, \"memory_copy_ms\": 213.505}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "2643d2895079614de1baede4a377f805f0ae0a35",
          "message": "Merge pull request #351 from alias2k/fix/index-delete-retry-344\n\ntest(engine): wait for the seed marker before checking the dropped generation",
          "timestamp": "2026-11-16T02:23:49+01:00",
          "tree_id": "c04de0d61405d49047125061ba705ef2ff5559a8",
          "url": "https://github.com/alias2k/flusso/commit/2643d2895079614de1baede4a377f805f0ae0a35"
        },
        "date": 1794793191336,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 1181.9142698366184,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 40.031, \"disk_write_ms\": 318.949, \"memory_copy_ms\": 153.264}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V45 96-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 1076.5095181394997,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 40.031, \"disk_write_ms\": 318.949, \"memory_copy_ms\": 153.264}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V45 96-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 914.441082413835,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 40.031, \"disk_write_ms\": 318.949, \"memory_copy_ms\": 153.264}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V45 96-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 1268.0286807807458,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 40.031, \"disk_write_ms\": 318.949, \"memory_copy_ms\": 153.264}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V45 96-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "9ff2371c13404e88aeb7eab5ab2c918788077c17",
          "message": "Merge pull request #352 from alias2k/fix/postgres-test-image-342\n\nfix(test): every test and bench starts Postgres through one home",
          "timestamp": "2026-11-16T02:56:16+01:00",
          "tree_id": "6b646b402f2bbc32d9abf20a0e127a88b272b5e3",
          "url": "https://github.com/alias2k/flusso/commit/9ff2371c13404e88aeb7eab5ab2c918788077c17"
        },
        "date": 1794795342122,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 704.919464963673,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.44, \"disk_write_ms\": 337.58, \"memory_copy_ms\": 191.09}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 741.3088708303062,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.44, \"disk_write_ms\": 337.58, \"memory_copy_ms\": 191.09}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 571.8146743042698,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.44, \"disk_write_ms\": 337.58, \"memory_copy_ms\": 191.09}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 935.7741676724561,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.44, \"disk_write_ms\": 337.58, \"memory_copy_ms\": 191.09}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "4c874552e9d7f4079eb7f9c8aab9bf1faad27d01",
          "message": "Merge pull request #353 from alias2k/fix/bench-gate-identical-binaries-346\n\nfix(ci): the PR bench gate skips benches identical on both sides",
          "timestamp": "2026-11-16T03:40:35+01:00",
          "tree_id": "58993d5ba634ac3a835578657cfe81f933d808e3",
          "url": "https://github.com/alias2k/flusso/commit/4c874552e9d7f4079eb7f9c8aab9bf1faad27d01"
        },
        "date": 1794798057347,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 550.5205041994888,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.425, \"disk_write_ms\": 136.137, \"memory_copy_ms\": 223.204}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 506.39268975827764,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.425, \"disk_write_ms\": 136.137, \"memory_copy_ms\": 223.204}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 409.91543949353616,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.425, \"disk_write_ms\": 136.137, \"memory_copy_ms\": 223.204}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 610.858386362662,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.425, \"disk_write_ms\": 136.137, \"memory_copy_ms\": 223.204}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "edd9802db7909afec26b02dd4fb99a8a9f997259",
          "message": "Merge pull request #356 from alias2k/docs/coordinator-review-ab\n\ndocs(coordinator): name just review-ab, and split long runs to fit one call",
          "timestamp": "2026-11-16T04:05:46+01:00",
          "tree_id": "8ca997f7064ce172a2670dac1c84abce746fe261",
          "url": "https://github.com/alias2k/flusso/commit/edd9802db7909afec26b02dd4fb99a8a9f997259"
        },
        "date": 1794799301874,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 1148.1247022510802,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 40.109, \"disk_write_ms\": 319.015, \"memory_copy_ms\": 153.933}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V45 96-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 1066.6471678042749,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 40.109, \"disk_write_ms\": 319.015, \"memory_copy_ms\": 153.933}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V45 96-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 875.5844470860072,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 40.109, \"disk_write_ms\": 319.015, \"memory_copy_ms\": 153.933}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V45 96-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 1312.431184819815,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 40.109, \"disk_write_ms\": 319.015, \"memory_copy_ms\": 153.933}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V45 96-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0b7243403c98bc940d3e35e277b6a8de165fe965",
          "message": "Merge pull request #357 from alias2k/ci/postgres-version-matrix-348\n\nci: run the Postgres e2e suites on 14 and 18 (nightly, on demand, by label)",
          "timestamp": "2026-11-16T06:51:31+01:00",
          "tree_id": "64ec26ba21fc231944e5ee03491fa8c84f95d734",
          "url": "https://github.com/alias2k/flusso/commit/0b7243403c98bc940d3e35e277b6a8de165fe965"
        },
        "date": 1794809325045,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 995.4534470553752,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 43.673, \"disk_write_ms\": 78.961, \"memory_copy_ms\": 208.02}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6975P-C\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 1024.5272798341673,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 43.673, \"disk_write_ms\": 78.961, \"memory_copy_ms\": 208.02}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6975P-C\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 800.8623984951167,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 43.673, \"disk_write_ms\": 78.961, \"memory_copy_ms\": 208.02}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6975P-C\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 1285.8130249125911,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 43.673, \"disk_write_ms\": 78.961, \"memory_copy_ms\": 208.02}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6975P-C\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "678a68416ef44905e85308d536ce4cf562c99698",
          "message": "Merge pull request #359 from alias2k/fix/scripted-reproducible-build-355\n\nfix(script): keep Rhai's ahash/runtime-rng so builds are reproducible",
          "timestamp": "2026-11-16T07:28:03+01:00",
          "tree_id": "888ba528803ea4aaf0a2a3e7b6cd10a50d2a2156",
          "url": "https://github.com/alias2k/flusso/commit/678a68416ef44905e85308d536ce4cf562c99698"
        },
        "date": 1794811702025,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 612.1660502847818,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.367, \"disk_write_ms\": 130.978, \"memory_copy_ms\": 213.003}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 516.0525182099155,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.367, \"disk_write_ms\": 130.978, \"memory_copy_ms\": 213.003}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 414.795816818313,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.367, \"disk_write_ms\": 130.978, \"memory_copy_ms\": 213.003}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 658.1085823760263,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.367, \"disk_write_ms\": 130.978, \"memory_copy_ms\": 213.003}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "36888c7fc76f6a664b8f6f377f5a949e8e3ac13d",
          "message": "Merge pull request #362 from alias2k/fix/bench-regression-issue-ownership-360\n\nfix(ci): bench-regression.py touches only the newest issue its bot opened",
          "timestamp": "2026-11-16T08:00:33+01:00",
          "tree_id": "64a2ba4654f6a51704aa431cba222477f3d86f18",
          "url": "https://github.com/alias2k/flusso/commit/36888c7fc76f6a664b8f6f377f5a949e8e3ac13d"
        },
        "date": 1794813431351,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 1204.8152286389486,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 42.508, \"disk_write_ms\": 321.326, \"memory_copy_ms\": 160.233}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V45 96-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 1054.9234433865704,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 42.508, \"disk_write_ms\": 321.326, \"memory_copy_ms\": 160.233}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V45 96-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 847.3516893480285,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 42.508, \"disk_write_ms\": 321.326, \"memory_copy_ms\": 160.233}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V45 96-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 1240.1133433565367,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 42.508, \"disk_write_ms\": 321.326, \"memory_copy_ms\": 160.233}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V45 96-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "f29fba59dae3df22deb956c0badc63e545520100",
          "message": "Merge pull request #361 from alias2k/feat/review-tooling-two-347\n\nfeat(bench): review-ab document digest and plan check (#347)",
          "timestamp": "2026-11-16T10:41:19+01:00",
          "tree_id": "13e6c06414ab8a064ecc90ad2ed190e3f7e76a1a",
          "url": "https://github.com/alias2k/flusso/commit/f29fba59dae3df22deb956c0badc63e545520100"
        },
        "date": 1794823816979,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 673.3808973146844,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.347, \"disk_write_ms\": 135.003, \"memory_copy_ms\": 218.018}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 442.8130555801716,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.347, \"disk_write_ms\": 135.003, \"memory_copy_ms\": 218.018}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 427.64370987528304,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.347, \"disk_write_ms\": 135.003, \"memory_copy_ms\": 218.018}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 666.4654236024822,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.347, \"disk_write_ms\": 135.003, \"memory_copy_ms\": 218.018}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "1c7b7e8cc0cfbafacbb5142bb915336b6ef7e58b",
          "message": "Merge pull request #365 from alias2k/feat/script-dates-364\n\nfeat(script): a real date type in script fields",
          "timestamp": "2026-11-16T16:07:13+01:00",
          "tree_id": "ef08c9767f3ad2351c246eedf780bdf358bff8e7",
          "url": "https://github.com/alias2k/flusso/commit/1c7b7e8cc0cfbafacbb5142bb915336b6ef7e58b"
        },
        "date": 1794843395727,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 657.51850086271,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.308, \"disk_write_ms\": 149.529, \"memory_copy_ms\": 220.363}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 491.565289103748,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.308, \"disk_write_ms\": 149.529, \"memory_copy_ms\": 220.363}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 420.52985545669463,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.308, \"disk_write_ms\": 149.529, \"memory_copy_ms\": 220.363}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 647.825062428674,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.308, \"disk_write_ms\": 149.529, \"memory_copy_ms\": 220.363}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "5854772b040b0c90ccc9e441cf5ac1d27c788371",
          "message": "Merge pull request #363 from alias2k/fix/read-after-visible-343\n\nfix(source-postgres)!: emit a change only once its transaction is visible",
          "timestamp": "2026-11-16T16:56:32+01:00",
          "tree_id": "891d30fac53e88fe07573630e3ef50f11e4d08be",
          "url": "https://github.com/alias2k/flusso/commit/5854772b040b0c90ccc9e441cf5ac1d27c788371"
        },
        "date": 1794845648000,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 1058.2604232512458,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 43.272, \"disk_write_ms\": 328.197, \"memory_copy_ms\": 160.961}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V45 96-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 882.337656573207,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 43.272, \"disk_write_ms\": 328.197, \"memory_copy_ms\": 160.961}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V45 96-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 820.805269663795,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 43.272, \"disk_write_ms\": 328.197, \"memory_copy_ms\": 160.961}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V45 96-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 1135.7937848056902,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 43.272, \"disk_write_ms\": 328.197, \"memory_copy_ms\": 160.961}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V45 96-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "69038a89948cc710757a2c335a70c4e14918b311",
          "message": "Merge pull request #367 from alias2k/fix/float-roundtrip-366\n\nfix(source-postgres): read and write every number exactly",
          "timestamp": "2026-11-16T19:41:18+01:00",
          "tree_id": "00fb725e4a169347e276cfadb27a5055b3ef34b0",
          "url": "https://github.com/alias2k/flusso/commit/69038a89948cc710757a2c335a70c4e14918b311"
        },
        "date": 1794856082426,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 875.3618038842601,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.499, \"disk_write_ms\": 338.696, \"memory_copy_ms\": 193.218}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 671.4115119777164,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.499, \"disk_write_ms\": 338.696, \"memory_copy_ms\": 193.218}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 574.2107021407151,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.499, \"disk_write_ms\": 338.696, \"memory_copy_ms\": 193.218}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 841.5206466921487,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.499, \"disk_write_ms\": 338.696, \"memory_copy_ms\": 193.218}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "d7fa61bffad65e18151e59e72fe516e4a944378b",
          "message": "Merge pull request #378 from alias2k/feat/fence-retry-377\n\nfeat(query): a fenced read rides out a restart of flusso",
          "timestamp": "2026-11-16T23:55:25+01:00",
          "tree_id": "aa19f653c36b96b966181c789b3171f84ca5d643",
          "url": "https://github.com/alias2k/flusso/commit/d7fa61bffad65e18151e59e72fe516e4a944378b"
        },
        "date": 1794871509754,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 693.478938519847,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 57.101, \"disk_write_ms\": 353.454, \"memory_copy_ms\": 210.749}, \"cores\": 4, \"cpu\": \"INTEL(R) XEON(R) PLATINUM 8573C\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 771.0901860887845,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 57.101, \"disk_write_ms\": 353.454, \"memory_copy_ms\": 210.749}, \"cores\": 4, \"cpu\": \"INTEL(R) XEON(R) PLATINUM 8573C\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 547.2659584055037,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 57.101, \"disk_write_ms\": 353.454, \"memory_copy_ms\": 210.749}, \"cores\": 4, \"cpu\": \"INTEL(R) XEON(R) PLATINUM 8573C\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 860.2221129874964,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 57.101, \"disk_write_ms\": 353.454, \"memory_copy_ms\": 210.749}, \"cores\": 4, \"cpu\": \"INTEL(R) XEON(R) PLATINUM 8573C\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "6c10f449174044d3def995ede65ec6161c8db08f",
          "message": "Merge pull request #375 from alias2k/fix/shutdown-371\n\nfix(cli)!: a stop drains in-flight changes under a deadline; the same sender twice forces",
          "timestamp": "2026-11-17T03:08:08+01:00",
          "tree_id": "4c883a0d407d9e4175c67e98c2812df0aa7708fa",
          "url": "https://github.com/alias2k/flusso/commit/6c10f449174044d3def995ede65ec6161c8db08f"
        },
        "date": 1794882332390,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 1116.0468269633043,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 40.001, \"disk_write_ms\": 322.524, \"memory_copy_ms\": 159.977}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V45 96-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 1061.2372950032388,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 40.001, \"disk_write_ms\": 322.524, \"memory_copy_ms\": 159.977}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V45 96-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 848.8379841971606,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 40.001, \"disk_write_ms\": 322.524, \"memory_copy_ms\": 159.977}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V45 96-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 1203.1108339770665,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 40.001, \"disk_write_ms\": 322.524, \"memory_copy_ms\": 159.977}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V45 96-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "eb54d865edf5c462ae665f34989adb453b96cc58",
          "message": "Merge pull request #384 from alias2k/feat/read-time-fields\n\nfeat(query): read-time fields: rules and computed fields over a request context",
          "timestamp": "2026-11-17T12:56:49+01:00",
          "tree_id": "446b1e8f51e7b563a4296e1d3409ec0913019583",
          "url": "https://github.com/alias2k/flusso/commit/eb54d865edf5c462ae665f34989adb453b96cc58"
        },
        "date": 1794918147623,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 821.3602486296897,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.622, \"disk_write_ms\": 338.078, \"memory_copy_ms\": 194.677}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 764.2462011646959,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.622, \"disk_write_ms\": 338.078, \"memory_copy_ms\": 194.677}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 578.798991653552,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.622, \"disk_write_ms\": 338.078, \"memory_copy_ms\": 194.677}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 910.6584633014602,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.622, \"disk_write_ms\": 338.078, \"memory_copy_ms\": 194.677}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "6d297b48040d4d6559f339f6c6cf3416b59a14e0",
          "message": "Merge pull request #389 from alias2k/fix/backfill-own-ids-385\n\nfix(engine): a snapshot row is its own document, not a change to resolve",
          "timestamp": "2026-11-17T14:29:30+01:00",
          "tree_id": "76b421230afea6ef823b1967bd7c302dad81bd4e",
          "url": "https://github.com/alias2k/flusso/commit/6d297b48040d4d6559f339f6c6cf3416b59a14e0"
        },
        "date": 1794923937358,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 589.8666964204866,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.395, \"disk_write_ms\": 136.928, \"memory_copy_ms\": 220.615}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 515.0093271255961,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.395, \"disk_write_ms\": 136.928, \"memory_copy_ms\": 220.615}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 3520.4200538316763,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.395, \"disk_write_ms\": 136.928, \"memory_copy_ms\": 220.615}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 505.7921624475579,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.395, \"disk_write_ms\": 136.928, \"memory_copy_ms\": 220.615}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "1c12b9a615bfb48b96a7194c182a052c11ad8efd",
          "message": "Merge pull request #392 from alias2k/design/per-index-tasks-386\n\ndocs(adr): 0012 each index runs on its own task (proposed, #386)",
          "timestamp": "2026-11-17T16:04:58+01:00",
          "tree_id": "b3bd5d08a4b26eb12d6bf5937c6ded44e6b53b21",
          "url": "https://github.com/alias2k/flusso/commit/1c12b9a615bfb48b96a7194c182a052c11ad8efd"
        },
        "date": 1794929093193,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 623.5852396235115,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 60.012, \"disk_write_ms\": 89.194, \"memory_copy_ms\": 193.293}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 655.5897002963758,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 60.012, \"disk_write_ms\": 89.194, \"memory_copy_ms\": 193.293}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 3516.629280106484,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 60.012, \"disk_write_ms\": 89.194, \"memory_copy_ms\": 193.293}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 588.3736193178743,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 60.012, \"disk_write_ms\": 89.194, \"memory_copy_ms\": 193.293}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "21794c74fc6e1cbdc9cb95f1e3d4a0c5a116e42f",
          "message": "Merge pull request #400 from alias2k/fix/item-429-and-release-395\n\nfix(sinks): resend items OpenSearch refuses as busy; release a lane delivery by its ticket",
          "timestamp": "2026-11-17T19:04:46+01:00",
          "tree_id": "b9dd0e7d408fcd398bf5da83f37fd3f1bf195edf",
          "url": "https://github.com/alias2k/flusso/commit/21794c74fc6e1cbdc9cb95f1e3d4a0c5a116e42f"
        },
        "date": 1794939818827,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 835.3893849856875,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.56, \"disk_write_ms\": 338.804, \"memory_copy_ms\": 190.243}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 699.3899773840765,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.56, \"disk_write_ms\": 338.804, \"memory_copy_ms\": 190.243}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 4085.124070640698,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.56, \"disk_write_ms\": 338.804, \"memory_copy_ms\": 190.243}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 876.7797512989907,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.56, \"disk_write_ms\": 338.804, \"memory_copy_ms\": 190.243}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "edd75f48369449ec87408801ad154dddab8c02ec",
          "message": "Merge pull request #401 from alias2k/feat/concurrent-staging-398\n\nStage a sink's indexes concurrently at start",
          "timestamp": "2026-11-17T20:22:07+01:00",
          "tree_id": "7ea675ee75c1975832e9f5752a8c4fa61e8f1a99",
          "url": "https://github.com/alias2k/flusso/commit/edd75f48369449ec87408801ad154dddab8c02ec"
        },
        "date": 1794945065237,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 637.7157806403305,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.44, \"disk_write_ms\": 141.053, \"memory_copy_ms\": 217.396}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 508.344673715772,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.44, \"disk_write_ms\": 141.053, \"memory_copy_ms\": 217.396}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 3390.730975018477,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.44, \"disk_write_ms\": 141.053, \"memory_copy_ms\": 217.396}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 524.6792815496261,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.44, \"disk_write_ms\": 141.053, \"memory_copy_ms\": 217.396}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "da3f53fead075742874f14a09d10279ab7f7067d",
          "message": "Merge pull request #409 from alias2k/feat/stage-timings-399\n\nfeat!: per-stage timings in /metrics, and an attribution table in flusso-bench",
          "timestamp": "2026-11-17T21:25:05+01:00",
          "tree_id": "0e2cd11a04f1e76b2b081ef0ff2d04bab90038f3",
          "url": "https://github.com/alias2k/flusso/commit/da3f53fead075742874f14a09d10279ab7f7067d"
        },
        "date": 1794948292203,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 647.5121772758839,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.394, \"disk_write_ms\": 136.305, \"memory_copy_ms\": 219.98}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 516.9221835326715,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.394, \"disk_write_ms\": 136.305, \"memory_copy_ms\": 219.98}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 3361.322785528166,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.394, \"disk_write_ms\": 136.305, \"memory_copy_ms\": 219.98}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 536.2438980654895,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.394, \"disk_write_ms\": 136.305, \"memory_copy_ms\": 219.98}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "e64011b1c255968f7a1863284dcef35979eed9d0",
          "message": "Merge pull request #405 from alias2k/ci/docs-only-skip-402\n\nci: a docs-only pull request skips what documentation can't break",
          "timestamp": "2026-11-17T21:49:23+01:00",
          "tree_id": "0a7904db446022e890e25b59053dca3baa05bc24",
          "url": "https://github.com/alias2k/flusso/commit/e64011b1c255968f7a1863284dcef35979eed9d0"
        },
        "date": 1794949689422,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 731.5200435707384,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.551, \"disk_write_ms\": 331.474, \"memory_copy_ms\": 198.187}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 698.6972841740488,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.551, \"disk_write_ms\": 331.474, \"memory_copy_ms\": 198.187}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 4713.158374086254,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.551, \"disk_write_ms\": 331.474, \"memory_copy_ms\": 198.187}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 733.0606620699378,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.551, \"disk_write_ms\": 331.474, \"memory_copy_ms\": 198.187}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "8412ea63bf2ab98e1fbd1bed68715dfcb87ef3c8",
          "message": "Merge pull request #416 from alias2k/fix/ci-docs-only-402-followup\n\nci: the drift guard knows the bench's attribution report is no doc read",
          "timestamp": "2026-11-17T22:17:44+01:00",
          "tree_id": "8c637ccff7b857de8f8c1dd65df2eba777c770b2",
          "url": "https://github.com/alias2k/flusso/commit/8412ea63bf2ab98e1fbd1bed68715dfcb87ef3c8"
        },
        "date": 1794951456440,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 682.7829411566781,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 60.142, \"disk_write_ms\": 87.84, \"memory_copy_ms\": 192.42}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 641.9330099110585,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 60.142, \"disk_write_ms\": 87.84, \"memory_copy_ms\": 192.42}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 3717.6668000490363,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 60.142, \"disk_write_ms\": 87.84, \"memory_copy_ms\": 192.42}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 609.7064187668038,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 60.142, \"disk_write_ms\": 87.84, \"memory_copy_ms\": 192.42}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "9f3611b6521743d43947f1060fc515361db0a135",
          "message": "Merge pull request #417 from alias2k/bench/live-drain-deterministic\n\nbench(engine): live_drain measures a sink that takes time",
          "timestamp": "2026-11-18T04:00:43+01:00",
          "tree_id": "a74305d10e0fdb36559ea007415eeb41b0dfc2e4",
          "url": "https://github.com/alias2k/flusso/commit/9f3611b6521743d43947f1060fc515361db0a135"
        },
        "date": 1794971984585,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 811.2147269011793,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.338, \"disk_write_ms\": 338.784, \"memory_copy_ms\": 187.913}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 762.0277368009124,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.338, \"disk_write_ms\": 338.784, \"memory_copy_ms\": 187.913}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 4064.4056107468523,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.338, \"disk_write_ms\": 338.784, \"memory_copy_ms\": 187.913}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 710.9834826010665,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.338, \"disk_write_ms\": 338.784, \"memory_copy_ms\": 187.913}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "ffd54d22f9864ff01ed035f82d6b61f42fb2c3ca",
          "message": "Merge pull request #408 from alias2k/feat/concurrent-writes-386-s2\n\nfeat(engine): concurrent writes per sink (ADR 0012 stage 2)",
          "timestamp": "2026-11-18T09:34:59+01:00",
          "tree_id": "d379d5db34d2f1f7ffaaffcd5cd1044ab1e15aa4",
          "url": "https://github.com/alias2k/flusso/commit/ffd54d22f9864ff01ed035f82d6b61f42fb2c3ca"
        },
        "date": 1794992007960,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 1127.0674031947703,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 41.25, \"disk_write_ms\": 299.835, \"memory_copy_ms\": 216.045}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6973P-C\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 920.906835146788,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 41.25, \"disk_write_ms\": 299.835, \"memory_copy_ms\": 216.045}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6973P-C\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 6453.4898762451385,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 41.25, \"disk_write_ms\": 299.835, \"memory_copy_ms\": 216.045}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6973P-C\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 935.1832736065749,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 41.25, \"disk_write_ms\": 299.835, \"memory_copy_ms\": 216.045}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6973P-C\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "t@e.x",
            "name": "Jakub Wasilczyk"
          },
          "committer": {
            "email": "t@e.x",
            "name": "Jakub Wasilczyk"
          },
          "distinct": true,
          "id": "bb3104ae27461ba7eb74c22d15d3f19895f0e48a",
          "message": "docs(skills): the coordinator defines done first, measures late on a quiet system, lands in slices",
          "timestamp": "2026-11-19T09:13:18+01:00",
          "tree_id": "20ea67cbb2ffd65cc50e514278bf9fdbe3f769ee",
          "url": "https://github.com/alias2k/flusso/commit/bb3104ae27461ba7eb74c22d15d3f19895f0e48a"
        },
        "date": 1795077054640,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 1277.9641781183968,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 43.44, \"disk_write_ms\": 74.361, \"memory_copy_ms\": 197.145}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6975P-C\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 1107.2148897514808,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 43.44, \"disk_write_ms\": 74.361, \"memory_copy_ms\": 197.145}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6975P-C\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 6631.682157307461,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 43.44, \"disk_write_ms\": 74.361, \"memory_copy_ms\": 197.145}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6975P-C\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 1142.500274281844,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 43.44, \"disk_write_ms\": 74.361, \"memory_copy_ms\": 197.145}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6975P-C\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "31822a83bc7907eec46ea2cf54f1722bb6b47e2d",
          "message": "Merge pull request #422 from alias2k/feat/concurrency-adr-0012\n\nfeat(engine)!: concurrent builds and writes, a memory budget, one lane per index and sink (ADR 0012 stages 3–7)",
          "timestamp": "2026-11-19T10:14:26+01:00",
          "tree_id": "69ceef36770ff799a35fa15bd7b0f818af6c827f",
          "url": "https://github.com/alias2k/flusso/commit/31822a83bc7907eec46ea2cf54f1722bb6b47e2d"
        },
        "date": 1795080866521,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 1033.059552372858,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.478, \"disk_write_ms\": 329.036, \"memory_copy_ms\": 189.624}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 716.1672537259857,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.478, \"disk_write_ms\": 329.036, \"memory_copy_ms\": 189.624}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 6109.674601243588,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.478, \"disk_write_ms\": 329.036, \"memory_copy_ms\": 189.624}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 813.7347972380834,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.478, \"disk_write_ms\": 329.036, \"memory_copy_ms\": 189.624}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "c58f05534ea8bb8f2bc5a47782b58096f8d5f312",
          "message": "Merge pull request #424 from alias2k/test/merge-wait-guards\n\ntest(engine): pin the one-lane merge-wait guard and the locked refresh case",
          "timestamp": "2026-11-19T10:57:09+01:00",
          "tree_id": "1c6608c0a2e41c6da6207d0b5f1b6acae512d2ca",
          "url": "https://github.com/alias2k/flusso/commit/c58f05534ea8bb8f2bc5a47782b58096f8d5f312"
        },
        "date": 1795083515334,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 694.4365283502688,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.376, \"disk_write_ms\": 130.987, \"memory_copy_ms\": 214.246}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 477.44147920664585,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.376, \"disk_write_ms\": 130.987, \"memory_copy_ms\": 214.246}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 4648.360519758411,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.376, \"disk_write_ms\": 130.987, \"memory_copy_ms\": 214.246}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 558.6814576887804,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.376, \"disk_write_ms\": 130.987, \"memory_copy_ms\": 214.246}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0d9c1271677556fbe0df23a8fb61e0f2f867bd06",
          "message": "Merge pull request #337 from alias2k/release-plz-2026-11-15T01-39-12Z\n\nchore: release v0.18.0",
          "timestamp": "2026-11-19T11:35:52+01:00",
          "tree_id": "260f392f2c025aa29e304d59e328519e3c4a9112",
          "url": "https://github.com/alias2k/flusso/commit/0d9c1271677556fbe0df23a8fb61e0f2f867bd06"
        },
        "date": 1795085764134,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 908.4056867069875,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.687, \"disk_write_ms\": 341.775, \"memory_copy_ms\": 190.439}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 658.0180600361751,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.687, \"disk_write_ms\": 341.775, \"memory_copy_ms\": 190.439}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 5923.908973781875,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.687, \"disk_write_ms\": 341.775, \"memory_copy_ms\": 190.439}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 825.532103258483,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.687, \"disk_write_ms\": 341.775, \"memory_copy_ms\": 190.439}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "ed0b0623e28017664ab2037e4f3d44d65fe47396",
          "message": "Merge pull request #429 from alias2k/docs/upgrade-0-18-intro\n\ndocs(operate): the 0.18 upgrade page opens with grouped lists, not one paragraph",
          "timestamp": "2026-11-19T15:27:08+01:00",
          "tree_id": "d61ca961daa03609859429598f3ae6340bb97d3f",
          "url": "https://github.com/alias2k/flusso/commit/ed0b0623e28017664ab2037e4f3d44d65fe47396"
        },
        "date": 1795100209766,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 801.9016076606829,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 55.394, \"disk_write_ms\": 352.443, \"memory_copy_ms\": 205.032}, \"cores\": 4, \"cpu\": \"INTEL(R) XEON(R) PLATINUM 8573C\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 802.4780895593753,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 55.394, \"disk_write_ms\": 352.443, \"memory_copy_ms\": 205.032}, \"cores\": 4, \"cpu\": \"INTEL(R) XEON(R) PLATINUM 8573C\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 5457.883085458931,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 55.394, \"disk_write_ms\": 352.443, \"memory_copy_ms\": 205.032}, \"cores\": 4, \"cpu\": \"INTEL(R) XEON(R) PLATINUM 8573C\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 842.8506338179894,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 55.394, \"disk_write_ms\": 352.443, \"memory_copy_ms\": 205.032}, \"cores\": 4, \"cpu\": \"INTEL(R) XEON(R) PLATINUM 8573C\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "540e1d3491b5ac39a39de4d5b334993a7b7469bf",
          "message": "Merge pull request #430 from alias2k/chore/coordinator-docs-agent\n\ndocs(skills): a documentation agent writes the manual before a PR is ready",
          "timestamp": "2026-11-19T15:41:24+01:00",
          "tree_id": "bdf9b4f1f610b655091697cd717bff922d4fd778",
          "url": "https://github.com/alias2k/flusso/commit/540e1d3491b5ac39a39de4d5b334993a7b7469bf"
        },
        "date": 1795101440009,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 897.902487398383,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.418, \"disk_write_ms\": 334.545, \"memory_copy_ms\": 186.335}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 759.2106642628723,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.418, \"disk_write_ms\": 334.545, \"memory_copy_ms\": 186.335}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 5749.853001298439,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.418, \"disk_write_ms\": 334.545, \"memory_copy_ms\": 186.335}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 813.0004678947794,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.418, \"disk_write_ms\": 334.545, \"memory_copy_ms\": 186.335}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "32a3dde3e1f00b7fb8fa86acf19aa945cc4a0465",
          "message": "Merge pull request #427 from alias2k/feat/lane-parking-404\n\nfeat(engine)!: a lane parks alone on a refusal or a write block (stage 9a)",
          "timestamp": "2026-11-19T16:27:10+01:00",
          "tree_id": "aabb666dc8b05edfdcbce9c299270adc28992cd2",
          "url": "https://github.com/alias2k/flusso/commit/32a3dde3e1f00b7fb8fa86acf19aa945cc4a0465"
        },
        "date": 1795103187336,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 1258.056224077547,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 43.626, \"disk_write_ms\": 76.107, \"memory_copy_ms\": 201.492}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6975P-C\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 1152.4197202686507,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 43.626, \"disk_write_ms\": 76.107, \"memory_copy_ms\": 201.492}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6975P-C\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 7573.965002223006,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 43.626, \"disk_write_ms\": 76.107, \"memory_copy_ms\": 201.492}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6975P-C\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 1300.0984136445734,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 43.626, \"disk_write_ms\": 76.107, \"memory_copy_ms\": 201.492}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6975P-C\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "3297b8c1d7179b46f62fefc4a289db485fec4c8f",
          "message": "Merge pull request #432 from alias2k/fix/derive-clippy-clean-431\n\nfix(query-derive): a flattened field in a fragment no longer trips clippy::assertions_on_constants",
          "timestamp": "2026-11-19T17:37:41+01:00",
          "tree_id": "71afdf7c8ebcfeab0e9cbb6016700350875df0eb",
          "url": "https://github.com/alias2k/flusso/commit/3297b8c1d7179b46f62fefc4a289db485fec4c8f"
        },
        "date": 1795107385584,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 1321.0029761860196,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 44.699, \"disk_write_ms\": 330.923, \"memory_copy_ms\": 170.614}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V45 96-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 929.9248861857687,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 44.699, \"disk_write_ms\": 330.923, \"memory_copy_ms\": 170.614}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V45 96-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 7961.670807285955,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 44.699, \"disk_write_ms\": 330.923, \"memory_copy_ms\": 170.614}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V45 96-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 934.145503471241,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 44.699, \"disk_write_ms\": 330.923, \"memory_copy_ms\": 170.614}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V45 96-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "31d8440494141a6ec7636c5874f39cd5c8a6f9b3",
          "message": "Merge pull request #434 from alias2k/feat/backoff-config\n\nfeat: configurable engine backoff; /status says refused",
          "timestamp": "2026-11-19T21:37:55+01:00",
          "tree_id": "8eece65103fc75beaf3c429416b75efd6c915d69",
          "url": "https://github.com/alias2k/flusso/commit/31d8440494141a6ec7636c5874f39cd5c8a6f9b3"
        },
        "date": 1795121999548,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 909.6855581354591,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 56.66, \"disk_write_ms\": 350.816, \"memory_copy_ms\": 203.611}, \"cores\": 4, \"cpu\": \"INTEL(R) XEON(R) PLATINUM 8573C\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 744.8616787876348,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 56.66, \"disk_write_ms\": 350.816, \"memory_copy_ms\": 203.611}, \"cores\": 4, \"cpu\": \"INTEL(R) XEON(R) PLATINUM 8573C\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 4870.072802853435,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 56.66, \"disk_write_ms\": 350.816, \"memory_copy_ms\": 203.611}, \"cores\": 4, \"cpu\": \"INTEL(R) XEON(R) PLATINUM 8573C\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 751.4883915905918,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 56.66, \"disk_write_ms\": 350.816, \"memory_copy_ms\": 203.611}, \"cores\": 4, \"cpu\": \"INTEL(R) XEON(R) PLATINUM 8573C\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "a187cf91e6d819ca90dcab927166fb9c937d4010",
          "message": "Merge pull request #435 from alias2k/fix/generation-deleted-mid-backfill-428\n\nfix(engine): a backfill whose target was created anew no longer seeds it",
          "timestamp": "2026-11-20T01:18:38+01:00",
          "tree_id": "4e3a16aaaf1971ba356a0a3054c0a707296abe72",
          "url": "https://github.com/alias2k/flusso/commit/a187cf91e6d819ca90dcab927166fb9c937d4010"
        },
        "date": 1795135050776,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 1150.494949456315,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 43.49, \"disk_write_ms\": 77.136, \"memory_copy_ms\": 199.487}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6975P-C\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 1281.1702754028158,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 43.49, \"disk_write_ms\": 77.136, \"memory_copy_ms\": 199.487}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6975P-C\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 7512.228058301493,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 43.49, \"disk_write_ms\": 77.136, \"memory_copy_ms\": 199.487}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6975P-C\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 1203.8362165619974,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 43.49, \"disk_write_ms\": 77.136, \"memory_copy_ms\": 199.487}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6975P-C\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "61c8109ee2b542225a0dbbcdba13db7856914da9",
          "message": "Merge pull request #436 from alias2k/feat/concurrency-8a-reader\n\nrefactor(engine)!: stage 8a slice 1, key lane items per index",
          "timestamp": "2026-11-20T03:09:15+01:00",
          "tree_id": "353f395431dc36012f7142251eaacda96b60e42e",
          "url": "https://github.com/alias2k/flusso/commit/61c8109ee2b542225a0dbbcdba13db7856914da9"
        },
        "date": 1795141719124,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 1049.2132422042669,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 43.563, \"disk_write_ms\": 78.628, \"memory_copy_ms\": 202.909}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6975P-C\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 1299.7430875714276,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 43.563, \"disk_write_ms\": 78.628, \"memory_copy_ms\": 202.909}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6975P-C\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 7411.904479942708,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 43.563, \"disk_write_ms\": 78.628, \"memory_copy_ms\": 202.909}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6975P-C\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 1222.2538147072837,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 43.563, \"disk_write_ms\": 78.628, \"memory_copy_ms\": 202.909}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6975P-C\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "b76baa3eca276f349e127a2bc68d5150976cfc0e",
          "message": "Merge pull request #439 from alias2k/feat/concurrency-8a-pending\n\nfeat(engine)!: stage 8a slice 2, pending documents per index",
          "timestamp": "2026-11-20T12:08:22+01:00",
          "tree_id": "2a42ac40f43b6b8200c49308941458fb3f281007",
          "url": "https://github.com/alias2k/flusso/commit/b76baa3eca276f349e127a2bc68d5150976cfc0e"
        },
        "date": 1795174065273,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 1125.9856422992243,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 44.035, \"disk_write_ms\": 78.792, \"memory_copy_ms\": 210.895}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6975P-C\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 1267.9465455915465,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 44.035, \"disk_write_ms\": 78.792, \"memory_copy_ms\": 210.895}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6975P-C\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 7023.508934134007,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 44.035, \"disk_write_ms\": 78.792, \"memory_copy_ms\": 210.895}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6975P-C\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 1225.429309827014,
            "unit": "changes/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 44.035, \"disk_write_ms\": 78.792, \"memory_copy_ms\": 210.895}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6975P-C\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "4cb0a4a63bc03aaef88411e37293cff4691d6f72",
          "message": "Merge pull request #440 from alias2k/feat/concurrency-8a-committed\n\nfeat(engine): changes_committed counts what every lane of a sink landed",
          "timestamp": "2026-11-20T14:33:02+01:00",
          "tree_id": "8f1fc82943274688a79055be3d58cd79bce0a6eb",
          "url": "https://github.com/alias2k/flusso/commit/4cb0a4a63bc03aaef88411e37293cff4691d6f72"
        },
        "date": 1795182782886,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 954.1781255249634,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.479, \"disk_write_ms\": 338.25, \"memory_copy_ms\": 191.384}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 720.5483377235365,
            "unit": "changes/s",
            "extra": "{\"images\": {\"drain\": \"every-lane-landed\", \"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.479, \"disk_write_ms\": 338.25, \"memory_copy_ms\": 191.384}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 5916.364513127911,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.479, \"disk_write_ms\": 338.25, \"memory_copy_ms\": 191.384}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 810.3768553118862,
            "unit": "changes/s",
            "extra": "{\"images\": {\"drain\": \"every-lane-landed\", \"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.479, \"disk_write_ms\": 338.25, \"memory_copy_ms\": 191.384}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "494be84c078fcb23a4815e45182c4fe27dce81e1",
          "message": "Merge pull request #441 from alias2k/feat/concurrency-8a-budget\n\nfeat(engine): share the memory budget per index, for many reservers",
          "timestamp": "2026-11-20T19:53:54+01:00",
          "tree_id": "b8bc36079bb65fb1c51ec17fa81b10a1b9b99da4",
          "url": "https://github.com/alias2k/flusso/commit/494be84c078fcb23a4815e45182c4fe27dce81e1"
        },
        "date": 1795202041442,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 927.1218521655421,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.697, \"disk_write_ms\": 338.648, \"memory_copy_ms\": 191.076}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 717.2585206288961,
            "unit": "changes/s",
            "extra": "{\"images\": {\"drain\": \"every-lane-landed\", \"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.697, \"disk_write_ms\": 338.648, \"memory_copy_ms\": 191.076}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 5812.7093788779475,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.697, \"disk_write_ms\": 338.648, \"memory_copy_ms\": 191.076}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 796.8756148554164,
            "unit": "changes/s",
            "extra": "{\"images\": {\"drain\": \"every-lane-landed\", \"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.697, \"disk_write_ms\": 338.648, \"memory_copy_ms\": 191.076}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "b042e77b5b058073285719215d61e52debb6c643",
          "message": "Merge pull request #443 from alias2k/feat/concurrency-8a-permits\n\nfeat(engine): waiting lines for build permits, and a key-read permit",
          "timestamp": "2026-11-21T01:15:38+01:00",
          "tree_id": "aea373268672a2e9d5100d9ccf631649ffe9911d",
          "url": "https://github.com/alias2k/flusso/commit/b042e77b5b058073285719215d61e52debb6c643"
        },
        "date": 1795221238588,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 1376.7872871729942,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 41.275, \"disk_write_ms\": 322.685, \"memory_copy_ms\": 154.911}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V45 96-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 1081.7274915621992,
            "unit": "changes/s",
            "extra": "{\"images\": {\"drain\": \"every-lane-landed\", \"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 41.275, \"disk_write_ms\": 322.685, \"memory_copy_ms\": 154.911}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V45 96-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 8222.135152934774,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 41.275, \"disk_write_ms\": 322.685, \"memory_copy_ms\": 154.911}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V45 96-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 1030.3012995063932,
            "unit": "changes/s",
            "extra": "{\"images\": {\"drain\": \"every-lane-landed\", \"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 41.275, \"disk_write_ms\": 322.685, \"memory_copy_ms\": 154.911}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V45 96-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "637c2ab35b23d9f03299a2f05cf4ec2128b6abf6",
          "message": "Merge pull request #445 from alias2k/ci/split-tests\n\nci: split the clippy + tests suite into parallel parts",
          "timestamp": "2026-11-21T03:35:55+01:00",
          "tree_id": "b2ce2b7cd3940d2ff66a3e6ad3a4258ddeb0e942",
          "url": "https://github.com/alias2k/flusso/commit/637c2ab35b23d9f03299a2f05cf4ec2128b6abf6"
        },
        "date": 1795230009983,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 1238.6640370448492,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 40.493, \"disk_write_ms\": 321.933, \"memory_copy_ms\": 163.603}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V45 96-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 1038.5436152198217,
            "unit": "changes/s",
            "extra": "{\"images\": {\"drain\": \"every-lane-landed\", \"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 40.493, \"disk_write_ms\": 321.933, \"memory_copy_ms\": 163.603}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V45 96-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 7623.585413170765,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 40.493, \"disk_write_ms\": 321.933, \"memory_copy_ms\": 163.603}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V45 96-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 985.3543769078575,
            "unit": "changes/s",
            "extra": "{\"images\": {\"drain\": \"every-lane-landed\", \"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 40.493, \"disk_write_ms\": 321.933, \"memory_copy_ms\": 163.603}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V45 96-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "7e10c43cd6d11111b1e1eed3704503781c887855",
          "message": "Merge pull request #446 from alias2k/feat/concurrency-8a-reader\n\nfeat(engine): stage 8a slice 6a: a reader task ahead of one builder, the lead, the stop barrier",
          "timestamp": "2026-11-21T11:49:20+01:00",
          "tree_id": "079ffb5d9d33d28b17e75b5c7fa1b78022b03955",
          "url": "https://github.com/alias2k/flusso/commit/7e10c43cd6d11111b1e1eed3704503781c887855"
        },
        "date": 1795259480763,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 733.4734133571702,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.428, \"disk_write_ms\": 135.045, \"memory_copy_ms\": 220.372}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 467.7991462720067,
            "unit": "changes/s",
            "extra": "{\"images\": {\"drain\": \"every-lane-landed\", \"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.428, \"disk_write_ms\": 135.045, \"memory_copy_ms\": 220.372}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 4326.534095540631,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.428, \"disk_write_ms\": 135.045, \"memory_copy_ms\": 220.372}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 574.9749114373948,
            "unit": "changes/s",
            "extra": "{\"images\": {\"drain\": \"every-lane-landed\", \"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.428, \"disk_write_ms\": 135.045, \"memory_copy_ms\": 220.372}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "dafa4990edc121705e844b880b21321d7111f1f1",
          "message": "Merge pull request #448 from alias2k/feat/concurrency-8a-index-tasks\n\nfeat(engine): stage 8a slice 6b-1a, a frontier per index and routed",
          "timestamp": "2026-11-21T14:51:48+01:00",
          "tree_id": "f9b9626f76ad7d01a70bedb3d1de3ee5690a8883",
          "url": "https://github.com/alias2k/flusso/commit/dafa4990edc121705e844b880b21321d7111f1f1"
        },
        "date": 1795270348214,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 939.3638347898572,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.367, \"disk_write_ms\": 329.762, \"memory_copy_ms\": 192.009}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 674.7805120655715,
            "unit": "changes/s",
            "extra": "{\"images\": {\"drain\": \"every-lane-landed\", \"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.367, \"disk_write_ms\": 329.762, \"memory_copy_ms\": 192.009}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 6018.2564769265855,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.367, \"disk_write_ms\": 329.762, \"memory_copy_ms\": 192.009}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 812.273530277733,
            "unit": "changes/s",
            "extra": "{\"images\": {\"drain\": \"every-lane-landed\", \"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.367, \"disk_write_ms\": 329.762, \"memory_copy_ms\": 192.009}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "5d1e54823c53a2c4ed1290487fb34f84d6d5ed4c",
          "message": "Merge pull request #450 from alias2k/feat/concurrency-8a-dispatcher\n\nfeat(engine): stage 8a slice 6b-1b, the dispatcher and together for N",
          "timestamp": "2026-11-21T17:12:18+01:00",
          "tree_id": "82538f850bc9d6e5df3a05bb7cbdfac92f9963bb",
          "url": "https://github.com/alias2k/flusso/commit/5d1e54823c53a2c4ed1290487fb34f84d6d5ed4c"
        },
        "date": 1795278830003,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 858.8321323433383,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 56.826, \"disk_write_ms\": 358.57, \"memory_copy_ms\": 208.699}, \"cores\": 4, \"cpu\": \"INTEL(R) XEON(R) PLATINUM 8573C\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 787.0437762006915,
            "unit": "changes/s",
            "extra": "{\"images\": {\"drain\": \"every-lane-landed\", \"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 56.826, \"disk_write_ms\": 358.57, \"memory_copy_ms\": 208.699}, \"cores\": 4, \"cpu\": \"INTEL(R) XEON(R) PLATINUM 8573C\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 5360.506953517769,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 56.826, \"disk_write_ms\": 358.57, \"memory_copy_ms\": 208.699}, \"cores\": 4, \"cpu\": \"INTEL(R) XEON(R) PLATINUM 8573C\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 804.6736532766704,
            "unit": "changes/s",
            "extra": "{\"images\": {\"drain\": \"every-lane-landed\", \"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 56.826, \"disk_write_ms\": 358.57, \"memory_copy_ms\": 208.699}, \"cores\": 4, \"cpu\": \"INTEL(R) XEON(R) PLATINUM 8573C\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "f993dbd3e07b3f7cd8c54de9e6ea3a9664305d6e",
          "message": "Merge pull request #451 from alias2k/feat/concurrency-8a-builders\n\nfeat(engine): stage 8a slice 6b-1c, a builder per index",
          "timestamp": "2026-11-21T22:12:55+01:00",
          "tree_id": "4530b7983cfe83027cffa1abd61b0b56a8355732",
          "url": "https://github.com/alias2k/flusso/commit/f993dbd3e07b3f7cd8c54de9e6ea3a9664305d6e"
        },
        "date": 1795296761984,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 1070.925456195115,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.739, \"disk_write_ms\": 337.753, \"memory_copy_ms\": 190.973}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 745.8637650429055,
            "unit": "changes/s",
            "extra": "{\"images\": {\"drain\": \"every-lane-landed\", \"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.739, \"disk_write_ms\": 337.753, \"memory_copy_ms\": 190.973}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 6337.716215737544,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.739, \"disk_write_ms\": 337.753, \"memory_copy_ms\": 190.973}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 788.8641084216465,
            "unit": "changes/s",
            "extra": "{\"images\": {\"drain\": \"every-lane-landed\", \"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.739, \"disk_write_ms\": 337.753, \"memory_copy_ms\": 190.973}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "ac8109a66df0ff634b8c254c46fc256d3a89a617",
          "message": "Merge pull request #452 from alias2k/feat/concurrency-8a-exempt\n\nfeat(engine): a held index stops no other index's reading (stage 8a slice 6b-2)",
          "timestamp": "2026-11-22T00:20:36+01:00",
          "tree_id": "cf98218eeb893bbc0998cde50e9b3cd4131b58ae",
          "url": "https://github.com/alias2k/flusso/commit/ac8109a66df0ff634b8c254c46fc256d3a89a617"
        },
        "date": 1795304370563,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 1110.8235787476217,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 40.114, \"disk_write_ms\": 327.239, \"memory_copy_ms\": 158.084}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V45 96-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 954.2055442387968,
            "unit": "changes/s",
            "extra": "{\"images\": {\"drain\": \"every-lane-landed\", \"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 40.114, \"disk_write_ms\": 327.239, \"memory_copy_ms\": 158.084}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V45 96-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 7477.262262334378,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 40.114, \"disk_write_ms\": 327.239, \"memory_copy_ms\": 158.084}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V45 96-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 975.4051945699343,
            "unit": "changes/s",
            "extra": "{\"images\": {\"drain\": \"every-lane-landed\", \"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 40.114, \"disk_write_ms\": 327.239, \"memory_copy_ms\": 158.084}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V45 96-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "5abd535c36ff1bf8e287bcb709f4e54bb8108acd",
          "message": "Merge pull request #453 from alias2k/feat/concurrency-8a-walks\n\nfeat(engine): stage 8a slice 6b-3: walks at once, key reads on their own task",
          "timestamp": "2026-11-22T03:57:01+01:00",
          "tree_id": "eed8df19552dd798a5bdf99364ea393a1fc27099",
          "url": "https://github.com/alias2k/flusso/commit/5abd535c36ff1bf8e287bcb709f4e54bb8108acd"
        },
        "date": 1795317363074,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 1209.4523722115678,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 43.706, \"disk_write_ms\": 76.241, \"memory_copy_ms\": 200.67}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6975P-C\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 1211.5333528238498,
            "unit": "changes/s",
            "extra": "{\"images\": {\"drain\": \"every-lane-landed\", \"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 43.706, \"disk_write_ms\": 76.241, \"memory_copy_ms\": 200.67}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6975P-C\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 6932.094822791435,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 43.706, \"disk_write_ms\": 76.241, \"memory_copy_ms\": 200.67}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6975P-C\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 1281.491666020264,
            "unit": "changes/s",
            "extra": "{\"images\": {\"drain\": \"every-lane-landed\", \"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 43.706, \"disk_write_ms\": 76.241, \"memory_copy_ms\": 200.67}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6975P-C\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "6f2b950c0d7bc9b37a7251b0dff78df41b586ac9",
          "message": "Merge pull request #454 from alias2k/feat/concurrency-8a-status\n\nfeat(engine,cli): stage 8a slice 7: per-index ingest status, metrics, routes and the backlog alert",
          "timestamp": "2026-11-22T04:30:25+01:00",
          "tree_id": "9bc523ad654d783aa33bc1d9d2266d34bc7f3c51",
          "url": "https://github.com/alias2k/flusso/commit/6f2b950c0d7bc9b37a7251b0dff78df41b586ac9"
        },
        "date": 1795319426869,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 1005.459324900876,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.994, \"disk_write_ms\": 338.141, \"memory_copy_ms\": 187.351}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 645.0483637363421,
            "unit": "changes/s",
            "extra": "{\"images\": {\"drain\": \"every-lane-landed\", \"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.994, \"disk_write_ms\": 338.141, \"memory_copy_ms\": 187.351}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 5203.2329088585775,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.994, \"disk_write_ms\": 338.141, \"memory_copy_ms\": 187.351}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 754.4097413658982,
            "unit": "changes/s",
            "extra": "{\"images\": {\"drain\": \"every-lane-landed\", \"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.994, \"disk_write_ms\": 338.141, \"memory_copy_ms\": 187.351}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "90073906bc35c7075dba7b881ed736982d699d8e",
          "message": "Merge pull request #456 from alias2k/feat/concurrency-8a-restart\n\nfeat(engine): restart one index alone and report it (stage 8a, slice 8)",
          "timestamp": "2026-11-22T10:55:07+01:00",
          "tree_id": "ed89953465778e90b9d708faf7d06654f49ce5ee",
          "url": "https://github.com/alias2k/flusso/commit/90073906bc35c7075dba7b881ed736982d699d8e"
        },
        "date": 1795342460587,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 1197.1402584215678,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 43.534, \"disk_write_ms\": 82.017, \"memory_copy_ms\": 204.742}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6975P-C\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 1278.4847783696193,
            "unit": "changes/s",
            "extra": "{\"images\": {\"drain\": \"every-lane-landed\", \"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 43.534, \"disk_write_ms\": 82.017, \"memory_copy_ms\": 204.742}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6975P-C\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 6923.800801983803,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 43.534, \"disk_write_ms\": 82.017, \"memory_copy_ms\": 204.742}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6975P-C\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 1244.813478626068,
            "unit": "changes/s",
            "extra": "{\"images\": {\"drain\": \"every-lane-landed\", \"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 43.534, \"disk_write_ms\": 82.017, \"memory_copy_ms\": 204.742}, \"cores\": 4, \"cpu\": \"Intel(R) Xeon(R) 6975P-C\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "6aa85913e337bd9a5f1b2b0938b8e9f53068e56c",
          "message": "Merge pull request #458 from alias2k/feat/lane-failure-2\n\nfeat(engine): fresh builds for a refused lane (stage 9 second half, slice 1)",
          "timestamp": "2026-11-23T15:46:51+01:00",
          "tree_id": "805c616134d4f01e32d2d973303c0197151c6f46",
          "url": "https://github.com/alias2k/flusso/commit/6aa85913e337bd9a5f1b2b0938b8e9f53068e56c"
        },
        "date": 1795446434733,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 819.0559016841203,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.593, \"disk_write_ms\": 337.429, \"memory_copy_ms\": 193.977}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 659.5442274462906,
            "unit": "changes/s",
            "extra": "{\"images\": {\"drain\": \"every-lane-landed\", \"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.593, \"disk_write_ms\": 337.429, \"memory_copy_ms\": 193.977}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 5760.8223551165975,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.593, \"disk_write_ms\": 337.429, \"memory_copy_ms\": 193.977}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 779.5992991170582,
            "unit": "changes/s",
            "extra": "{\"images\": {\"drain\": \"every-lane-landed\", \"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.593, \"disk_write_ms\": 337.429, \"memory_copy_ms\": 193.977}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "t@e.x",
            "name": "Jakub Wasilczyk"
          },
          "committer": {
            "email": "t@e.x",
            "name": "Jakub Wasilczyk"
          },
          "distinct": true,
          "id": "9b70cd6ec8479e324ab2ae697bc939417e1b52a7",
          "message": "perf(engine): a lane item carries its build's own ids, not a key per envelope\n\nEnvelope.key (d054ce47) made every envelope 24 bytes wider, and each sink\nengine walked the wider stride on every pass, so two sinks paid it twice.\nThe envelope is 90073906's again. Each lane item now holds, by refcount,\nthe id vector its build was handed (Batch.ids, never serialized); a\nparked item's keys are found in it by rendered id and index, and an item\nwhose envelopes don't all match is probed itself (F2). DocumentId moves to\nthe kernel. The builder keeps lending its id vectors and takes each back\nonce no item holds it, so a build allocates no vector in steady state.",
          "timestamp": "2026-11-23T17:31:51+01:00",
          "tree_id": "ebcd5e2792927b3457f91ced07d449386442c012",
          "url": "https://github.com/alias2k/flusso/commit/9b70cd6ec8479e324ab2ae697bc939417e1b52a7"
        },
        "date": 1795452862724,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 724.0833499686354,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.595, \"disk_write_ms\": 136.611, \"memory_copy_ms\": 227.127}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 462.24675406112453,
            "unit": "changes/s",
            "extra": "{\"images\": {\"drain\": \"every-lane-landed\", \"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.595, \"disk_write_ms\": 136.611, \"memory_copy_ms\": 227.127}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 4038.015891755347,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.595, \"disk_write_ms\": 136.611, \"memory_copy_ms\": 227.127}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 534.9541532165457,
            "unit": "changes/s",
            "extra": "{\"images\": {\"drain\": \"every-lane-landed\", \"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 54.595, \"disk_write_ms\": 136.611, \"memory_copy_ms\": 227.127}, \"cores\": 4, \"cpu\": \"AMD EPYC 7763 64-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "989c27caf9c367447c034eaa4770613cb6e7baec",
          "message": "Merge pull request #470 from alias2k/fix/lane-failure-followup\n\nfix(engine): follow-up to the lane-failure slice 2 review on main",
          "timestamp": "2026-11-23T22:14:43+01:00",
          "tree_id": "4cca9777dffcf9ef45cc8fd856a272aa40daee3a",
          "url": "https://github.com/alias2k/flusso/commit/989c27caf9c367447c034eaa4770613cb6e7baec"
        },
        "date": 1795469723910,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 1004.0302119096476,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.965, \"disk_write_ms\": 327.32, \"memory_copy_ms\": 193.476}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 701.8306661377954,
            "unit": "changes/s",
            "extra": "{\"images\": {\"drain\": \"every-lane-landed\", \"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.965, \"disk_write_ms\": 327.32, \"memory_copy_ms\": 193.476}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 5729.770988077197,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.965, \"disk_write_ms\": 327.32, \"memory_copy_ms\": 193.476}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 839.6900008109509,
            "unit": "changes/s",
            "extra": "{\"images\": {\"drain\": \"every-lane-landed\", \"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.965, \"disk_write_ms\": 327.32, \"memory_copy_ms\": 193.476}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "4f82c898ec67ff3fa3a1283a6a1400b12d11cc30",
          "message": "Merge pull request #462 from alias2k/fix/engine-bench-spawn-447\n\nfix(bench): the engine bench spawns the ingest engine, as the daemon does",
          "timestamp": "2026-11-23T22:53:30+01:00",
          "tree_id": "2488c9d1d194606da61708ed4112bbcdb4d62f02",
          "url": "https://github.com/alias2k/flusso/commit/4f82c898ec67ff3fa3a1283a6a1400b12d11cc30"
        },
        "date": 1795472151466,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 752.5225815527666,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 60.536, \"disk_write_ms\": 141.395, \"memory_copy_ms\": 225.037}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 529.140032315112,
            "unit": "changes/s",
            "extra": "{\"images\": {\"drain\": \"every-lane-landed\", \"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 60.536, \"disk_write_ms\": 141.395, \"memory_copy_ms\": 225.037}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 4307.344292742662,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 60.536, \"disk_write_ms\": 141.395, \"memory_copy_ms\": 225.037}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 586.5237922162298,
            "unit": "changes/s",
            "extra": "{\"images\": {\"drain\": \"every-lane-landed\", \"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 60.536, \"disk_write_ms\": 141.395, \"memory_copy_ms\": 225.037}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "89390131+JakubWasilczyk-Alias2k@users.noreply.github.com",
            "name": "Jakub Wasilczyk",
            "username": "JakubWasilczyk-Alias2k"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "359efa855ed449460708a3d631477a6c40a4c1df",
          "message": "Merge pull request #473 from alias2k/feat/lane-failure-4\n\nfeat(sinks): a closed index parks its lane blocked",
          "timestamp": "2026-11-23T23:23:51+01:00",
          "tree_id": "2e5faa4bbc48aa221696eb58ffd4cfa1ef266dcd",
          "url": "https://github.com/alias2k/flusso/commit/359efa855ed449460708a3d631477a6c40a4c1df"
        },
        "date": 1795473833229,
        "tool": "customBiggerIsBetter",
        "benches": [
          {
            "name": "complex/ci/backfill_docs_per_s",
            "value": 949.6410913164505,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.629, \"disk_write_ms\": 336.462, \"memory_copy_ms\": 191.075}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "complex/ci/drain_changes_per_s",
            "value": 707.122852087764,
            "unit": "changes/s",
            "extra": "{\"images\": {\"drain\": \"every-lane-landed\", \"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.629, \"disk_write_ms\": 336.462, \"memory_copy_ms\": 191.075}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 4000, \"items_per_order\": 4, \"name\": \"ci\", \"orders\": 0, \"orders_per_user\": 5, \"probe_rate_per_s\": 20, \"probes\": 150, \"products\": 0, \"reviews_per_product\": 0, \"rss_cap_mib\": 2048, \"tags\": 8, \"tags_per_user\": 4, \"users\": 2000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/backfill_docs_per_s",
            "value": 6071.335081821432,
            "unit": "docs/s",
            "extra": "{\"images\": {\"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.629, \"disk_write_ms\": 336.462, \"memory_copy_ms\": 191.075}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          },
          {
            "name": "reference/ci/drain_changes_per_s",
            "value": 837.3585540627895,
            "unit": "changes/s",
            "extra": "{\"images\": {\"drain\": \"every-lane-landed\", \"opensearch\": \"opensearchproject/opensearch:2\", \"postgres\": \"postgres:16-alpine\"}, \"runner\": {\"calibration\": {\"cpu_sha256_ms\": 46.629, \"disk_write_ms\": 336.462, \"memory_copy_ms\": 191.075}, \"cores\": 4, \"cpu\": \"AMD EPYC 9V74 80-Core Processor\"}, \"scale\": {\"burst\": 10000, \"items_per_order\": 3, \"name\": \"ci\", \"orders\": 25000, \"orders_per_user\": 0, \"probe_rate_per_s\": 20, \"probes\": 200, \"products\": 1000, \"reviews_per_product\": 2, \"rss_cap_mib\": 2048, \"tags\": 0, \"tags_per_user\": 0, \"users\": 5000, \"wall_cap_secs\": 900, \"writers\": 8}}"
          }
        ]
      }
    ]
  }
}