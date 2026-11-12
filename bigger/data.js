window.BENCHMARK_DATA = {
  "lastUpdate": 1794527237084,
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
      }
    ]
  }
}