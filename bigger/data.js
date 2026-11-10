window.BENCHMARK_DATA = {
  "lastUpdate": 1794308055235,
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
      }
    ]
  }
}