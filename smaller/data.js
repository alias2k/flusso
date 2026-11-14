window.BENCHMARK_DATA = {
  "lastUpdate": 1794645025245,
  "repoUrl": "https://github.com/alias2k/flusso",
  "entries": {
    "flusso (smaller is better)": [
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
        "date": 1788626128761,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 2971544.5882352944,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 170162819.75,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 64708626.46875,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 77983373.78571428,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 178472825.60000002,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 73499504.36666667,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 88120521.56944445,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 530995.4628378379,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 7433170.071428571,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 101939429,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 104938340.83333334,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 11296118.375,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 189770.52951217417,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 247.17751406260965,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 192163.22750277864,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 2041963020,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 280299013,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 484045659.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 210537745.66666666,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 260772.42461192812,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 267739.4959719832,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 307768.82451923075,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 622406.1164422396,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 20298970.667948715,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 38662311.16842105,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 79776614,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 248218055,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 20435990.8,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 1128895453.25,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 118945804.89285713,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 72129531.54583333,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 221865.12280701753,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 253.41347155008754,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 71.238478,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 231.761539,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 90.03125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 2,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 21.163594470046085,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 246.4130434782608,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 72.491643,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 175.702281,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 122.14453125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 21,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 65.87591240875913,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 462.4999999999998,
            "unit": "ms",
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
        "date": 1788702242609,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 4512389,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 110196111.875,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 55437891.425,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 83863684.74375,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 124192521.94642857,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 72218040.76785713,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 74352016.2625,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 843448.8420745921,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 10521547.9,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 138701774.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 104003394.1875,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 10520555.42857143,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 281708.9556083303,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 428.83959609172166,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 278827.6916245404,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1405354188.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 362731302.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 478021326.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 208203609.3333333,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 366102.2993236715,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 379195.9794014769,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 422972.49040904467,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 792142.8203781513,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 13054272.992424242,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 21897651.366666667,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 53640418.59926471,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 299406584.75,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 16044320.944444444,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 279691633.6111111,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 92992248.62037037,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 63490226.97916667,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 317100.02435897436,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 432.2559312987353,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 71.77856,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 360.832847,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 83.31640625,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 3,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 20.931818181818183,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 304.49999999999875,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 71.076898,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 162.177673,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 86.6640625,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 33,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 72.67699115044249,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 476.5000000000005,
            "unit": "ms",
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
        "date": 1788703251936,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3798916.5357142854,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 147943329.08333334,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 53069414.8375,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 80832958.64285713,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 161450523.93333334,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 70308974.92222223,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 83911192.23214287,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 804398.2132850242,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 9163430.75,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 119081960.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 115867298.37142858,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 9485570.5,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 219184.96610449735,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 373.1295853545696,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 218638.93311546842,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1182717720.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 318685287.75,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 431019743.75,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 208129669.66666666,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 305674.6806451613,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 314562.3096296296,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 372450.67077063385,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 725309.2793040293,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 11214890.065079365,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 18244199.16694079,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 52801404.68181819,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 282761336.5,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 15166530.722222222,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 261767284.81666666,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 88094994.46666667,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 63330142.75,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 248020.6507352941,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 376.2339238845144,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 73.12740199999999,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 225.671682,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 90.4140625,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 3,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 20.170623145400594,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 271.8750000000014,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 72.21519699999999,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 156.86273,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 95.47265625,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 27,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 71.44670050761421,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 479.79166666666606,
            "unit": "ms",
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
        "date": 1788770199679,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 4355253.583333334,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 114985981.08333334,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 50774924.87142857,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 67801370.78571428,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 124709273.04166666,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 61183500.92063492,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 64688360.83333333,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 876953.2063953488,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 10434270.4,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 131702393,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 92064341.66666666,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 11129392.25,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 341293.7773276749,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 402.3303489484531,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 337623.9230003827,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1257954514,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 348319979,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 434754418.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 213827370.6666667,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 388614.4062099359,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 393771.02006769826,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 457549.9244047619,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 848023.1133960048,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 13112057.409722222,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 23473936.57352941,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 52012240.49025974,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 297505590,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 15916178.388888888,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 296761764.375,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 98879129.25925925,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 65846912.84375,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 378120.4264647402,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 404.9790087156019,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 27.287424,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 121.565055,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 96.7734375,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 4,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 19.154589371980677,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 214.6315789473683,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 51.674527,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 191.18225999999999,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 75.8671875,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 36,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 60.44776119402985,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 465.8823529411757,
            "unit": "ms",
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
        "date": 1791453260502,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 4324087.458333333,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 109766739.25,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 48536305.675,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 66528848.85,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 130390842.375,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 62311797.75,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 66499102.03571428,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 884074.3921773143,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 11240997.8,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 135692965.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 93593225.55555555,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 12054691.5,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 371123.65090090095,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 412.8362119275415,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 368267.75073653203,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1307462401,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 377448886.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 490504290.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 232603286.75,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 421186.8572028844,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 424952.58415624313,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 488400.7347437509,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 893407.4390151515,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 16246404.63986014,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 28869857.9484127,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 62277806.599999994,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 292510218,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 18319416.714285716,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 313439242.5833334,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 101945212.73333333,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 70016851.1388889,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 409003.1671028325,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 414.2048542946119,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 28.289633000000002,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 171.15482200000002,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 77.5234375,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 4,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 19.681603773584907,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 228.0294117647058,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 49.147699,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 337.48641000000003,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 71.359375,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 37,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 59.909090909090914,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 466.13636363636346,
            "unit": "ms",
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
        "date": 1791533314937,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 4384669.833333333,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 114127340.95238096,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 48690369.75,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 67278885.48958334,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 130127756.8,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 61889077.25,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 66109048.26041667,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 903902.6273809525,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 10628495.4,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 134214789,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 92178426.26666667,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 11282476.416666668,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 375691.05723905726,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 478.69657100646896,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 376380.9260835913,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1180208542,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 334598682,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 433088495,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 216323964,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 424743.45709647494,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 430870.3254818594,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 486451.13983791514,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 871720.4046658986,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 13812891.25,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 26240879.222222224,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 57784824.2,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 297929743.25,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 17282084.5625,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 293607959.2708334,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 97307324.77777779,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 67877257.27678572,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 415333.2377561475,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 474.4648202564494,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 27.871916,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 114.393461,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 95.91796875,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 4,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 19.43471337579618,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 217.99999999999986,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 48.549665,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 160.56984899999998,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 84.54296875,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 37,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 58.71647509578545,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 465.4166666666665,
            "unit": "ms",
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
        "date": 1793964444655,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 2310002.6136363633,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 62658294.04166667,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 33373975.444444444,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 47337026.85416667,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 75360757.8125,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 40394592.56666666,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 44589002.902777776,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 442086.4967320261,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 5457820.15,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 84262505.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 59929101.21428572,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 6057643.833333334,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 130950.83863945579,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 203.66975007102639,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 129834.70578722267,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 586925079.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 179079249.3333333,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 219421397.6666667,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 116381577.625,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 188112.84464285715,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 189940.0166766827,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 224555.4203950811,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 458443.2758493031,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 7442372.684151785,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 10959845.042105263,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 32532676.375,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 171800459.83333334,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 9810441.107142858,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 156842707.05,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 52761225.46666667,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 38705477.01473923,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 164842.2621666433,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 212.11845622317998,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 25.689014999999998,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 95.454329,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 85.37109375,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 2,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 8.673469387755102,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 145.29999999999973,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 24.825052,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 69.20642600000001,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 69.640625,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 15,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 32.11722488038278,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 232.97058823529454,
            "unit": "ms",
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
        "date": 1794305425346,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 4032071.1923076925,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 135744032.9166667,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 46064359.45833333,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 89901002.375,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 158542425,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 46453698.89285714,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 113528036.4,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 844032.0554086803,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 11406239.4,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 130764826.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 172329351.3125,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 12325679.916666666,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 332446.2500964506,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 392.8019415532426,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 332664.81569549686,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1227997597.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 336524456.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 436898506.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 223540845.83333334,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 416357.60832816374,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 427245.03302611364,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 479567.242281841,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 854283.5206367925,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 14804940.740384616,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 24430093.43371212,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 57958909.5,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 305196392.75,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 18677970.5,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 339194148.9166666,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 107300166.95555556,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 69481357.13839287,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 352295.39877913427,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 408.0061378072356,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 28.941416,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 185.016494,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 89.8515625,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 4,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 20.080385852090032,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 222.0400000000002,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 52.755382000000004,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 372.100095,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 77.86328125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 37,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 64.42307692307693,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 365.27777777777715,
            "unit": "ms",
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
        "date": 1794306984972,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3549072.2666666666,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 227246402.16666666,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 49856300.87142857,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 130594582.70833334,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 220141066,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 50049646.625,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 116957351.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 694405.7050094355,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 9474475,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 121657219,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 184306525.0416667,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 8856549.6875,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 190757.00300127716,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 337.2291285890536,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 213116.60694244027,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1065452626,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 296496031,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 372480190.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 191409850.1666667,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 322286.084229885,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 291286.117369186,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 375630.3118882788,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 735099.1223245924,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 10373668.21875,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 17653576.185714286,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 47496528.27521008,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 255653003.25,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 13504574.4,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 279135758.4166666,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 85614283.98809524,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 62636000.24375,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 220499.48713290194,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 311.497207981439,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 28.84165,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 91.52687499999999,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 89.8203125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 3,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 17.7197265625,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 196.8333333333324,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 50.894492,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 113.17232800000001,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 80.94921875,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 25,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 55.43893129770992,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 341.24999999999756,
            "unit": "ms",
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
        "date": 1794308052381,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 4042090.6923076925,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 126278008.22222222,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 46040586.45,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 85112387.875,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 155677474.42222223,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 46505042.55555555,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 109510576.55,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 868694.9652560083,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 11917170.1,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 139717751.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 166296027.2875,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 11812008.416666666,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 326842.83045977016,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 540.6733319589846,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 324747.9039323046,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1275166968.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 335890049,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 456596858.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 215492072.3333333,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 408836.47297463176,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 418072.764183145,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 476367.0655716832,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 837188.9113065889,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 12818426.266666666,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 23861732.65,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 53705228.10454545,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 305172076.5,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 17247615.571428575,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 298860027.5,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 98195342.3,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 66380601.027777776,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 346846.61140433187,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 393.3521593522188,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 28.507693,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 393.713751,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 90.63671875,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 4,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 19.53689167974882,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 234.80000000000018,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 49.429348,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 244.473379,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 85.5078125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 36,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 62.53776435045317,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 351.2500000000003,
            "unit": "ms",
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
        "date": 1794319215570,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 2213988.1875,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 87030146.7361111,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 33387764.02222222,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 76536738.08928572,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 96081415.09523809,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 33094210.45767196,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 79641274.525,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 451145.84370199696,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 6174654.3125,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 91719088,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 115440047.25,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 7748251.611111111,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 145128.5394409938,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 259.3659239842726,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 138273.6109291085,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 849247518.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 224845938.33333334,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 290496110.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 134838951,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 205544.6057142857,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 206785.66172072693,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 240986.1885341074,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 480378.6985849971,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 8703712.153110048,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 12358360.47777778,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 32034793.35625,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 185332282.75,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 11600672.363636363,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 169479082.64375,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 62426623.56944445,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 45457947.266666666,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 163176.88025382604,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 213.3542640114966,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 28.73564,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 66.58934099999999,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 86.67578125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 2,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 15.524475524475525,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 143.19999999999993,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 27.782943,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 79.779117,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 63.86328125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 17,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 35.12168141592921,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 234.1500000000002,
            "unit": "ms",
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
        "date": 1794321914285,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 2173217,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 78751306.8125,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 32551846.71666667,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 77479547.125,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 92238463.3,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 33053796.814814813,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 82505628.9236111,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 434118.50925925927,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 5845610.444444444,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 86108989,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 121406882.3611111,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 6540389.681818182,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 141058.15647893772,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 255.68465969092028,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 135204.35189701896,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 781130346.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 219636799.3333333,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 276044922,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 130916067,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 207343.5072213501,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 203524.69764102565,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 239055.124437836,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 475334.7588095238,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 7569572.275568182,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 11099921.101190476,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 31828915.668128654,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 181792230.75,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 10641346,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 163889318.91875,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 56617019.84166667,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 39893709.099999994,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 163977.36709104938,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 198.64185148880892,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 26.463437,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 91.814167,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 90.8046875,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 2,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 9.18421052631579,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 144.85000000000014,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 25.798553,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 76.915523,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 62.6640625,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 16,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 32.28672985781991,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 237.31250000000017,
            "unit": "ms",
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
        "date": 1794322977238,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 4106851.4615384615,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 130171050.525,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 45562375.97354497,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 87969601.05555555,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 154985197.96666667,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 46432881.36875,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 102753730.8125,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 885103.2615740742,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 11192623.1,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 134740656,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 166208454.125,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 11439437.083333334,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 328901.5353150406,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 516.5773660197142,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 325369.919777893,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1190620325,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 338287026.75,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 446079974.75,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 212935608.3333333,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 408376.3999094203,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 418730.6795489029,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 476538.32158730156,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 834049.8020833333,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 13153212.729166668,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 23631284.35866013,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 60716218.3,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 302774164.75,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 16822238.375,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 281770289.6666666,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 97054931.125,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 67284629.76875,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 348176.3044733044,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 400.95228233711316,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 27.821524,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 192.972524,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 97.80078125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 4,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 19.672330097087382,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 218.43181818181822,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 49.29385,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 207.82425400000002,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 72.76171875,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 36,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 62.89062500000001,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 350.93750000000057,
            "unit": "ms",
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
        "date": 1794324059699,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3038268.9411764704,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 220010709.16666666,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 42574892.16666667,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 116055639.2625,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 212472772.16666666,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 43252837.607407406,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 105164765.51111111,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 624388.5983455882,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 8426207.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 108510396.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 151422219.625,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 7416371.722222222,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 166311.74192488263,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 343.122856811479,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 226904.7077294686,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 905078206,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 255808288.75,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 330703505.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 168156183.8333333,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 279388.03622482135,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 306237.1282077922,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 363523.3603059581,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 663160.2562613981,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 7860916.913602941,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 12469357.266666666,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 38498903.10681114,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 224838443.75,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 11206906.875,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 227096231.2,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 76481136.1625,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 55075693.20952381,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 194482.1077157803,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 265.2557280991107,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 28.599449,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 91.968704,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 90.5234375,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 3,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 12.858231707317074,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 194.8214285714288,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 27.569105,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 86.428368,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 91.02734375,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 22,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 41.37731481481481,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 246.30172413793116,
            "unit": "ms",
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
        "date": 1794325054736,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3313222.59375,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 111641814.95,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 38004373.21111111,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 89408582.95555556,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 118684146.91666666,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 38498112.3,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 80883196.525,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 694110.9416130628,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 8778566,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 111105727.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 163106528.39999998,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 9258349.75,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 198161.4054227941,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 371.8556523113453,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 197479.09988587652,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1410677046.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 369069367.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 446318942,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 181780647.3333333,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 290130.0342157842,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 281100.0554699538,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 323935.854169279,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 603934.5428571429,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 15672022.861111112,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 20087051.635167465,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 54314553.225,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 265523375.75,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 14250263.333333334,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 283295044.1666666,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 101449116.1111111,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 61168062.21111111,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 219448.07101648353,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 373.1905426986445,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 27.143542,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 184.419588,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 84.58984375,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 3,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 14.880136986301371,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 228.294117647059,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 46.588082,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 759.9309840000001,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 94.2265625,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 24,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 43.27956989247312,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 453.07692307692207,
            "unit": "ms",
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
        "date": 1794327174845,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 4215697.923076923,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 137042704.1388889,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 49364338.30555555,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 103419894.19444445,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 148710954.25,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 50190202.30555555,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 96637217.66666666,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 816426.9572876447,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 11381499.7,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 137585461,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 205242765,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 10816846.42857143,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 260633.47623804765,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 484.92483222468593,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 254126.0514784946,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1361889775,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 342208706,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 456857668.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 209543189.3333333,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 375479.7403708056,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 376557.9559015988,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 424352.1030927835,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 791055.0066465923,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 15115908.008333333,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 22638616.82777778,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 53026503.777777776,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 306399377.5,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 16187499.611111112,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 277083907.25,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 92861766.72222222,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 64866734.25694445,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 289584.89254385966,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 465.71214901024064,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 27.947787,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 360.743292,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 77.08984375,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 4,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 19.13915094339623,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 235.49999999999955,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 51.54054,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 342.596432,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 79.46484375,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 33,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 61.42857142857143,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 363.1250000000001,
            "unit": "ms",
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
        "date": 1794352505355,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3447707.4,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 234723048.33333334,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 48089088.29166667,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 125289782.50555556,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 226166641.3333333,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 48517062.575,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 120752493.25,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 681308.5113636364,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 9118030.666666668,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 116455659,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 183579536.33333334,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 8308234.166666666,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 181287.13929618767,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 385.202353985116,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 225944.5112962963,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 975968015.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 271166044.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 357432699.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 184750291.8333333,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 318587.84030257934,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 320682.66434776795,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 371981.1453649397,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 707711.722383901,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 8657351.920138888,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 17925313.34375,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 47156105.48863636,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 242160214.75,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 12446365.59090909,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 261685958.11666667,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 81984014.59166667,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 59596674.0625,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 216982.4659992785,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 395.3461982298078,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 28.079596,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 119.20603200000001,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 89.125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 3,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 13.671171171171173,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 192.92105263157882,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 28.514466,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 104.747546,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 81.48046875,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 24,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 48.11557788944724,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 248.97468354430384,
            "unit": "ms",
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
        "date": 1794354264565,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3258466.90625,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 110900082.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 37561487.24404762,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 86094373.69444445,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 116044465.73809524,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 38126925.819047615,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 80684181.1111111,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 624259.2342495988,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 8471686.416666666,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 112646800.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 152489100.025,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 10130151.75,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 206935.04892533936,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 361.8934289434915,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 203562.18057708873,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1203454809.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 312482162.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 478786618.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 162822963.8333333,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 299558.53469716496,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 302092.5164519592,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 344564.83734453504,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 611473.2931207004,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 9982417.896491228,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 20228136.916666664,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 42003607.85714286,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 250668340.25,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 12928814.4,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 271796019.54761904,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 76377917.70833334,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 57158158.94,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 233090.8020949944,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 343.99349004942576,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 27.508573000000002,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 361.352257,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 79.91796875,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 3,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 15.702479338842977,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 214.54545454545422,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 35.706331,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 92.33396400000001,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 80.2578125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 25,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 49.94131455399061,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 454.1666666666667,
            "unit": "ms",
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
        "date": 1794355307956,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 4012093.692307692,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 129079135.65,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 45862867.57539682,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 95112463.33333334,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 154510941.125,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 46356211.91071428,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 95925878.4,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 827728.5875903985,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 10817220,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 133834033,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 169016994.92857143,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 11125862.57142857,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 324246.19590336137,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 520.914178375479,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 325877.77782091097,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1264909899.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 343550625.75,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 438464088.75,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 216393965,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 418247.243664269,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 419818.6974688914,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 476514.60841836734,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 832629.4199585137,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 14211185.0625,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 19183037.849358976,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 56318978.700367644,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 302118976.75,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 16700840.214285715,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 286618630.1666666,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 96943200.85714287,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 67389533.4404762,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 348654.5182449495,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 680.8199767698014,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 27.950817,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 248.43659200000002,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 91.17578125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 3,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 18.882978723404253,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 224.28571428571453,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 50.163089,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 258.818906,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 71.91015625,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 33,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 58.55263157894737,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 408.61111111111217,
            "unit": "ms",
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
        "date": 1794388743222,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 2133068.4375,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 82948920.9,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 32299709.476190478,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 76002265.66666666,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 93217036.2936508,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 32381685.24074074,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 83108554.97222222,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 426307.44953703706,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 5888633.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 87323168.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 124704445.66666666,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 8029017,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 138345.79322250639,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 274.61475408290323,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 139262.3402061856,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 943103239.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 210675180.1666667,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 295961834.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 132846958.125,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 212649.1175196473,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 208357.7392764858,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 246941.0150293255,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 490231.1088905039,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 12107180.728787879,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 13402873.420634922,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 35111220.52923977,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 215840779.25,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 11963012.791666666,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 171235293.7388889,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 60672423.266666666,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 43837804.53125,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 165850.90696438064,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 395.293327526634,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 28.541703000000002,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 131.741475,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 89.0546875,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 2,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 14.65909090909091,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 161.2000000000006,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 28.318033999999997,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 89.256355,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 76.72265625,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 16,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 36.215753424657535,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 239.5416666666667,
            "unit": "ms",
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
        "date": 1794389879834,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3985966.615384615,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 129344718.94444445,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 46542173.41666667,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 98055645.76785713,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 156346521.69444445,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 47927115.58333333,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 113044918.52777778,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 863300.4613095238,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 11341000.2,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 132387566.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 172735499.9583333,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 12951446.25,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 334405.0566955267,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 511.77479712690877,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 328269.3843903941,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1221043585,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 347195025.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 448363726.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 233086056.8333333,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 412168.1248037957,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 416964.3335866261,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 475407.17864736257,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 832125.8818181818,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 15542589.241666667,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 25580369.791666664,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 58871751.66666667,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 310126959.5,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 19244181.07142857,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 298769973.6666666,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 103825473.03518519,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 71454683.06388888,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 353346.5162545788,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 704.9205813029422,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 29.20196,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 252.72699300000002,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 84.109375,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 4,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 19.488734835355285,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 227.5000000000002,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 51.67678,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 370.314965,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 65.4609375,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 35,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 57.625,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 414.25000000000125,
            "unit": "ms",
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
        "date": 1794392635109,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 4018502.153846154,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 134416991.4097222,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 46205021.638888896,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 89109315.9875,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 155287971.2916667,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 46478456.13333333,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 99279410.16666667,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 930045.5411158118,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 10849321.600000001,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 131219794,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 166425999.6388889,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 11596692.714285715,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 329909.1557603687,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 561.0374741495552,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 327533.4251452858,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1229961946.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 348172582.75,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 462870364.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 217804387.8333333,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 418864.20793650794,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 421795.58847053035,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 479740.9375,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 837602.5405982905,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 14217857.762820512,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 21687926.833333332,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 56031694.5,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 304286218.5,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 17130116.285714287,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 282627686.1,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 92922585.9,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 65701951.4375,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 349880.625,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 726.5552071418072,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 27.974189,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 271.526472,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 90.328125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 3,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 19.28448275862069,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 230.00000000000017,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 49.926915,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 286.696809,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 80.11328125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 34,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 54.973118279569896,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 430.62499999999915,
            "unit": "ms",
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
        "date": 1794396002592,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 4199277.153846154,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 139512324.42857143,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 49556437.315476194,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 116351613.63333333,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 148157284.3809524,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 49950542.7,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 105178708.85714287,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 818564.0142857142,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 11030063.8,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 134164629.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 205618046.6666667,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 11505974.57142857,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 258288.14853615517,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 500.36408762121664,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 256923.51546568627,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1370046972,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 349617734.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 467476577.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 213144339.33333334,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 374024.4025062211,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 377187.47729903203,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 423636.51785714284,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 784261.6605762625,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 14895456.0625,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 22001361.629807692,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 53450883.73611111,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 309937077.25,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 16433799.5,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 279871119.67857146,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 96403417.00595239,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 66388212.97321428,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 285211.97909652075,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 662.6454979020479,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 29.275994999999998,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 306.611832,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 80.6640625,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 3,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 19.352331606217618,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 226.7999999999995,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 51.622804,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 210.376111,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 84.02734375,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 30,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 49.13522012578617,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 460.9375,
            "unit": "ms",
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
        "date": 1794400823124,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3492788.966666667,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 236589900.3333333,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 48909305.59166667,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 127895800.1111111,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 220876497.66666666,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 49626778.95,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 121500174.55,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 689740.7854620083,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 9255296.083333332,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 120185266,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 187219568.1666667,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 8659479.9375,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 187900.3926923077,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 427.69025724963467,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 233501.15233753074,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1055548899.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 289462490.75,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 384266164,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 191297808.33333334,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 318943.5544871795,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 299866.5427272727,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 388033.7803030303,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 747479.5062500001,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 11138706.625,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 19427419.875,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 45676513.056680165,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 256346386.5,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 13160216.05,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 269986223.39285713,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 84290007.375,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 60953088.19270833,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 219238.43084733892,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 568.0255954742488,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 28.868661,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 92.468434,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 86.9296875,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 3,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 16.20224719101124,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 190.46153846153885,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 28.736501,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 111.86568500000001,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 84.25,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 23,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 46.29888268156425,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 360.5000000000018,
            "unit": "ms",
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
        "date": 1794407818397,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3312947.96875,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 108748277.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 37926049.983333334,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 88291079.84285715,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 115726364.25,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 38448331.16435185,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 86797664.6875,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 626421.8707298137,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 8493556.083333332,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 111267503,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 160699139.05555558,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 9678443.3125,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 205506.0227272727,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 409.9333083410533,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 202546.42284816247,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1104575418,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 276175496.75,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 380920646.75,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 165589364.16666666,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 293512.6477179659,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 296958.27267025085,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 336100.55647358444,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 611230.8819185644,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 11107348.625,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 18753025.586805556,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 42578063.01948052,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 244830903.25,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 13868035.850000001,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 234514090.5,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 73385116.42410713,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 54387919.233333334,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 229584.7186234818,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 516.1046650732003,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 27.049309,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 163.022967,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 90.21875,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 2,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 14.069602272727273,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 184.32142857142864,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 34.216652,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 274.79405299999996,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 68.6171875,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 23,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 46.82432432432432,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 326.87500000000114,
            "unit": "ms",
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
        "date": 1794410174166,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3661433.5714285714,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 125575247.05,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 46190847.375,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 91295000.54444444,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 157026049.625,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 46523982.76785714,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 110023348.84722222,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 896782.465860215,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 10997704.399999999,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 132817948,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 7491285.214285715,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 32016749.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 164340998.66666666,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 11724227.083333332,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 332456.5611742424,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 587.3357968401768,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 328728.45779220783,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1271994185.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 361741838.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 466941065.75,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 226161248.5,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 426101.56375,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 426172.525085034,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 487649.350140056,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 847819.1352513228,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 14363882.587912088,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 19947690.34411765,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 58857003.5,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 300130931.5,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 17674541.64285714,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 312716101.75,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 99127468.29166666,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 68922879.73809524,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 353080.9176767677,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 739.9876066408015,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 28.161203,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 360.25629100000003,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 90.21484375,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 3,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 19.326424870466322,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 230.16666666666669,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 51.217014,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 249.46312500000002,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 84.3046875,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 34,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 55.94660194174757,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 426.45833333333394,
            "unit": "ms",
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
        "date": 1794411289562,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3664346.1071428573,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 125147804.54166666,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 46206145.625,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 89607620.03571428,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 154918134.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 46518453.75,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 100299935.75,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 837397.9545454546,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 10780060.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 132080752.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 7443318.285714285,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 31888814,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 165507654.60714287,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 11790024.333333332,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 330027.51976495725,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 553.7467559340585,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 326992.8892339545,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1264913918.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 339902973.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 440210324.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 217846931.5,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 422838.2845030454,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 424634.04743008316,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 482525.765625,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 839087.7060944824,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 13754572.82828283,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 24469882.96666667,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 55817856.395588234,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 306312544.25,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 17138795.07142857,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 299778451.5833334,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 96500076.55833334,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 68251751.7375,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 348660.82272401435,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 735.8973929342992,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 27.982875,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 305.098577,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 93.328125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 3,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 19.094076655052262,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 218.68749999999972,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 49.548852999999994,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 238.71362,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 78.7421875,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 33,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 53.82513661202186,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 366.25000000000085,
            "unit": "ms",
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
        "date": 1794414209571,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 2013501.72,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 81526206.63095239,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 31280033,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 73863197.73333333,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 89031535.575,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 31666913.592592593,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 79091184.95833334,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 431254.6357142857,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 6203084.111111111,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 87570684.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 4014765.3333333335,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 15493949.666666668,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 117710562.13333333,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 7952044.611111111,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 136297.89725449472,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 306.7053499281434,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 135194.01953748008,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 891828636.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 211568717.6666667,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 287629650.75,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 132567755.75,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 198981.4266233766,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 199289.5284322034,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 231595.54451024078,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 456760.45457537856,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 9362153.018518519,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 13886311.719298245,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 33962118.266666666,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 191363788.75,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 11001252.40909091,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 161073972.30952382,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 57967533.3,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 43631572.275,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 159456.07198142415,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 409.6838335786375,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 28.111908,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 118.576529,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 93.49609375,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 2,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 11.457006369426752,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 135.1666666666669,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 27.135285,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 65.628529,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 69.2421875,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 15,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 32.08041958041959,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 237.0892857142859,
            "unit": "ms",
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
        "date": 1794416457725,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3117982.9117647056,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 108941978.94444445,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 37324097.66666667,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 81252611.08333334,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 116218349.33333334,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 37771635.077380955,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 81587731.08333334,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 640818.6616200828,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 8496209.416666666,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 107366603.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 5585464.611111111,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 22132858,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 162505563.25,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 9883355.5,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 201312.04708557256,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 431.23699561205103,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 198737.9536585366,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1132161530,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 291787908,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 401101646.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 183125736.8333333,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 285432.6531894576,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 288525.6333041958,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 331091.1393162393,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 606390.4796747968,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 11294413.389814815,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 22949689.37280702,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 47124492.71428572,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 239469901.75,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 15400135.714285715,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 334187759.63392854,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 78408738.0625,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 53786682.376190476,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 218176.03254676872,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 550.1366798574165,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 27.519683,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 246.730737,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 91.28125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 2,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 14.860703812316716,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 215.147058823529,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 36.994544999999995,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 168.862869,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 84.58984375,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 23,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 45.810810810810814,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 311.00000000000136,
            "unit": "ms",
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
        "date": 1794427901505,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3654830.1428571427,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 131979539.04761904,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 46475283.641666666,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 84513206.16666666,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 156042170.96666667,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 46868821.175,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 105558116.71111111,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 891500.4311827957,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 10887561.1,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 130017683.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 7456357.928571428,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 32394609,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 171680288.5,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 11888245.833333332,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 338531.8836805555,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 547.99300470151,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 334468.34126984124,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1395945703.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 363277183.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 499463711.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 225770782.5,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 428835.7734375,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 431652.830370858,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 489459.5369601329,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 854695.2892380618,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 15066303.05,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 24949649.583333336,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 61208930.8,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 307341718.5,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 17851652.1875,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 310091126.76666665,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 101820850.85555556,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 70326249.5625,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 358558.5934850513,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 720.3185921587512,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 28.235229,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 168.049961,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 92.3828125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 4,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 19.2948717948718,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 218.3571428571424,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 51.326056,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 597.966468,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 86.140625,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 34,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 56.60919540229885,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 417.50000000000114,
            "unit": "ms",
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
        "date": 1794436936560,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3666167.3571428573,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 135139070.2,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 46272310.15625,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 88353570.85,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 152965236.55,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 46648722.46875,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 108736589,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 840869.5265079365,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 10781275.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 132413733.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 7474403.857142857,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 31863112.25,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 168865292.5,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 12558127.083333334,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 334985.9853110048,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 556.0964256182735,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 331749.52863116574,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1274865388.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 339699496.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 461920975.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 214980079,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 422290.9198179272,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 427371.49256299256,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 484849.27185501065,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 843557.7311878109,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 14112602.811188811,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 21190590.875,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 56656299.177777775,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 319095631,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 17311544.25,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 286138228,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 96324513.22222222,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 67285072.71785715,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 353565.76992753625,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 755.6742724027349,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 27.962082,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 179.690719,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 91.078125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 3,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 18.76094570928196,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 226.29999999999998,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 49.724825,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 271.37931100000003,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 80.921875,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 34,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 55.440414507772026,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 416.4999999999992,
            "unit": "ms",
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
        "date": 1794438024140,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3410651.7666666666,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 245936862.6666667,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 50296594.625,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 147302638.2,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 230556098.3333333,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 50872131.44642857,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 124317646.65,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 727137.745,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 9571092.333333332,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 123115601,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 5996004.125,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 24236467.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 190360520,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 9174173.125,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 197068.6347188995,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 447.8715225666673,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 200259.44156720658,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1082044455.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 297195311,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 394566261.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 186609050,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 322606.7873665846,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 332864.04729254567,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 395322.803442029,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 767426.7270408163,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 9705475.61965812,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 17654459.83272059,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 46111298.68055555,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 264940306.25,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 13976958.2,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 257994914.5,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 90224128.75,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 64491554.37083334,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 226620.43245869776,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 591.5376811783837,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 29.878881999999997,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 159.87998100000002,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 91.21875,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 3,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 17.88391224862888,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 193.68181818181824,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 33.72847,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 142.56414,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 81.171875,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 25,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 48.736702127659576,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 305.5000000000007,
            "unit": "ms",
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
        "date": 1794439589673,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3690748.6785714286,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 126553369.92857143,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 45772396.361111104,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 98487792.63392857,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 154411340.3809524,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 46019051.09920634,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 99351524.33333334,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 846918.2902597403,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 10676851.1,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 129643135,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 7339207.857142857,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 31781733,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 165355555.1666667,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 11986491.583333332,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 335710.00695596094,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 528.2096227817071,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 332447.69266869605,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1289486185,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 344367394,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 446342271.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 213865552.66666666,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 416650.4654796512,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 431253.27202428685,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 485802.7238612836,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 852074.5673775426,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 13592777.21875,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 23131381.25,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 54378395.03333333,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 304858297.25,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 18246168,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 303460553.31666666,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 96483440.66666666,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 67828844.16666667,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 355904.7807017544,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 723.5685954430816,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 27.910335,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 369.594004,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 83.3125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 3,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 19.041450777202076,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 237.6470588235296,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 49.530879999999996,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 229.11173300000002,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 75.75,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 34,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 56.05263157894737,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 394.3750000000001,
            "unit": "ms",
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
        "date": 1794441190512,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 4015953.730769231,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 135533454.875,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 46363404.45555556,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 83808336.775,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 154296589.475,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 46614134.28125,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 102217634.2875,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 858784.458073149,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 11117936.7,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 129677848,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 7219435.142857143,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 32052278.25,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 168729877.13333333,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 12584583.166666668,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 338029.1380980861,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 533.4723200494284,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 330360.08562271064,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1194392788.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 340134959,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 446308593.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 211379457.33333334,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 414571.3074127907,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 426484.60291988414,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 486492.948538961,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 854045.0840996169,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 14676634.229166668,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 23594886.42105263,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 59915027.1,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 294959553.75,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 17864488.1875,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 306657817.75,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 96887105.36190476,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 69128150.2375,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 353679.24457817315,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 712.214459674231,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 28.472843,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 100.345586,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 91.8828125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 4,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 19.45205479452055,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 202.35294117647078,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 52.320601,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 207.691178,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 84.4296875,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 34,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 56.66666666666667,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 375.71428571428504,
            "unit": "ms",
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
        "date": 1794442245188,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3149355.46875,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 107101482.125,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 37565102.63690476,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 85918952.16666666,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 113032910.27380952,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 38014935.027777776,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 78335947.025,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 611179.5798180315,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 8560439.166666668,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 109575137,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 5144746.388888889,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 21769566,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 157289384.14285713,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 8514689.25,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 199266.91596736596,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 393.66043177692814,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 195074.61731601733,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1100470795.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 275149103.75,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 379547742,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 167337238.1666667,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 288114.8230536131,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 289407.4892590287,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 331012.3350018539,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 610008.1330769231,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 10568208.878787879,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 17190003.659090906,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 40433434.02631579,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 249782414.75,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 12868970.55,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 236577688.4166667,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 68520362.46875,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 55301113.07777777,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 220205.69488536156,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 531.7744952109836,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 26.646413,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 154.15913,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 83.046875,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 2,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 10.76086956521739,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 194.09090909090878,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 35.308966,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 248.44686199999998,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 95.4296875,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 23,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 47.767857142857146,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 248.0970149253731,
            "unit": "ms",
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
        "date": 1794443410335,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3906685.230769231,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 125572221.51666667,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 45850035.99166667,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 86590360.70833334,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 154534283.83035713,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 47235086.58333333,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 103933901.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 813983.8198678861,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 10970694.2,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 129040202.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 7033524.285714285,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 31314755.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 168902102.64285713,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 12305699.083333332,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 329426.1368888889,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 560.5837276594004,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 329489.7125783699,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1256239775.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 343600395.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 460112391,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 217020925,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 417145.40156250005,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 421267.72759433964,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 475466.2541208791,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 827737.8233695652,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 13608497.633333333,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 22819059.885620914,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 54321046.925,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 304032444.25,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 17456011.785714284,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 293323101.6666666,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 95992902.87777779,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 67604394.70982143,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 349550.8533862434,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 749.6907545618915,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 27.847675,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 205.47104299999998,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 89,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 4,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 19.56540447504303,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 205.71052631578937,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 49.116020000000006,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 249.417744,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 77.70703125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 34,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 55.33854166666667,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 394.06250000000045,
            "unit": "ms",
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
        "date": 1794481447908,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3847488.923076923,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 128194971.85,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 46175120.76190476,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 82605530.87857142,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 156967105.93055555,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 46453435.4125,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 97585842.66666666,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 828068.2029220778,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 11013531.1,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 134311525.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 7161466.142857143,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 32522714.75,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 169613401.46428573,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 12080765.785714285,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 328597.3520454962,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 531.6449873133258,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 323289.46892289934,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1210882679,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 344000040,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 455962807.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 220282966.1666667,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 408392.9820833333,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 421029.6874451755,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 477961.1622556391,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 841237.0337577161,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 13698428.18855219,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 25839777.376838237,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 57908672.083333336,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 298933994,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 17950292.07142857,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 296325798.35714287,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 97523386.57619047,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 67610471.4375,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 347061.76851851854,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 740.2572129954231,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 28.409261,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 213.251046,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 89.2109375,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 4,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 19.316608996539795,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 227.09090909090898,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 51.944392,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 230.075476,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 86.1015625,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 34,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 55.02793296089386,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 424.5454545454534,
            "unit": "ms",
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
        "date": 1794483057625,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3869297.846153846,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 132735973.92857143,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 45986457.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 83130571.55357143,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 156276111.9,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 46228914.52142857,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 105631510.7,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 829857.9989846923,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 11311166.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 135628234.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 7201977.357142857,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 32706000.25,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 168045661.8333333,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 13402994.5,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 339981.2937693327,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 547.1726004464285,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 337511.26062091504,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1238833289.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 339565747.75,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 461431130.75,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 232147884.3333333,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 432231.22840531566,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 434743.0155853039,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 488969.1105957534,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 853367.8122885835,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 15483780.8125,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 21482710.5,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 60248518.7,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 302580985,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 18969577.57142857,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 294313745.3333334,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 98496653.44444445,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 67828548.03125,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 360677.35555555555,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 750.9274166881344,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 29.246289,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 247.192651,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 79.5625,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 4,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 19.599999999999998,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 224.1999999999996,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 52.817147,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 219.508415,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 94.66015625,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 34,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 58.00970873786408,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 404.4444444444429,
            "unit": "ms",
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
        "date": 1794489128265,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 2172097.8125,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 86890378.30555555,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 32817000.277777776,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 72234697.19642857,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 92080388.25,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 33187621.766666666,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 81696178.25,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 451955.5170768682,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 6651089.6875,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 93821075.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 4006569.625,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 16525578.666666666,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 116670793.8,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 7136130.65,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 141686.86726190476,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 360.23722934501745,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 146380.5588760504,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 897457670.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 220807380.83333334,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 300638761.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 134168973.125,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 223633.57269038196,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 221277.798757764,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 250747.19475446426,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 487593.3113425926,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 9285175.022807017,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 16302643.871428572,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 35858445.375,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 200631781,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 11626373.416666668,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 172502400.475,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 59539478.900000006,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 44904448.43571429,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 167853.80630630633,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 443.60199011802024,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 27.819671,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 111.74046799999999,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 87.65625,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 2,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 11.06418918918919,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 135.0000000000004,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 27.700587,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 78.146733,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 79.12109375,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 16,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 34.81481481481482,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 240.99999999999986,
            "unit": "ms",
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
        "date": 1794490274617,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3866497.807692308,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 131952167.52380952,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 46456618.92857143,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 82572156.16666666,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 155377825.30952382,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 46840084.81666666,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 108513668,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 819525.8372513562,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 10849523,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 133915538.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 7333679.142857143,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 33055462,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 170498726.2222222,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 11815678.666666666,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 328817.02631578944,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 547.2303288925896,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 323884.3579892786,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1219079968.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 340447690.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 450665534.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 219996935.33333334,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 416676.04464285716,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 418058.67380536126,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 475892.3025412088,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 866332.9504830919,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 13965834.358058609,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 20942483.027960524,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 59126376.599999994,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 304823063.75,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 16388848.388888888,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 303088151.4583334,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 100096863.53333333,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 68839245.0625,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 346733.4316436252,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 747.3496684193822,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 28.666397,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 365.15268499999996,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 93.08984375,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 4,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 19.420103092783503,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 213.25000000000028,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 49.637145000000004,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 208.385819,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 78.98828125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 34,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 53.86740331491713,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 420.9090909090905,
            "unit": "ms",
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
        "date": 1794491254059,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 2138372.3125,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 83808571.40625,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 31940336.555555556,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 66901273.60069445,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 89186270.85625,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 33393524.241666667,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 76609762.625,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 433334.86362224293,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 6044920.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 90477233.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 4042134.125,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 16037026.666666668,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 113701897.96666667,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 8256860.9375,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 142961.3430451128,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 302.04994105570995,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 136320.96837002097,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 913557226,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 225628189.8333333,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 308957249.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 138037235.25,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 194987.74781746033,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 193494.43668981481,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 227656.67642857143,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 455080.96753246756,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 9401347.123626374,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 13862054.06439394,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 33974101.06818181,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 192187627.5,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 11460396.181818182,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 176718516.6597222,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 61221269.891666666,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 49819257.2037037,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 158885.33402380673,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 416.2982353118643,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 28.110948,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 94.897134,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 85.875,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 2,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 12.603857566765578,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 146.65000000000023,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 27.067047,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 211.414116,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 67.90625,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 16,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 32.993197278911566,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 239.58333333333334,
            "unit": "ms",
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
        "date": 1794492388998,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3860110.192307692,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 132191998.91666667,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 46306518.27083333,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 94916582.21875,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 156550058.75,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 46921161.03125,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 106032985.81944445,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 824270.9994736842,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 11619676.8,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 134014056,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 7265740.785714285,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 32048639.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 168505964.9,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 13183643.25,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 332669.62784313725,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 575.7054183024384,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 329808.6405890228,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1240976172,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 333140169.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 456639265.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 235628226.8333333,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 416590.37535014004,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 429181.3603896104,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 483125.11098595284,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 875832.6197552448,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 16597598.177884616,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 28238726.416666668,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 60847728.7,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 317346873.25,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 18273478.125,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 292357088.5208334,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 98959784,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 68495975.953125,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 354141.8862058081,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 754.1208246135054,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 28.76361,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 219.858089,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 93.0703125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 4,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 19.270657672849918,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 226.4772727272724,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 49.328976,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 155.821892,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 84.52734375,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 34,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 54.32432432432432,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 405.00000000000114,
            "unit": "ms",
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
        "date": 1794493515876,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3894291.384615385,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 126208695.85714287,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 45640545.46296296,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 93723702.25,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 155312276.5625,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 46131143.628472224,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 100772752.3888889,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 816068.3843483895,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 10785090.6,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 128186717.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 7293340.428571429,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 32284017.75,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 167499391.925,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 10840968.857142858,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 329109.25333667,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 539.9731699682363,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 324994.8969332298,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1278230773,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 345593626,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 459406064.75,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 218367608.5,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 415412.6021257837,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 416547.7313556763,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 473884.5891604889,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 832792.2237054085,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 13225508.444444444,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 21235701.80681818,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 52782060.777777776,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 307703453.75,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 16100548.777777778,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 301547491.4166666,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 96380010.06666666,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 67844856.125,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 347877.45474566636,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 765.7199815392465,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 27.589275999999998,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 211.62476999999998,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 82.91015625,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 3,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 19.12411971830986,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 201.62499999999966,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 48.150416,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 188.984806,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 84.38671875,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 34,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 53.58974358974359,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 406.1111111111112,
            "unit": "ms",
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
        "date": 1794494557982,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3162092.59375,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 110533223.75,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 37535804.57142857,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 86509105.26190476,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 115319814.31666666,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 37749269.233333334,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 79137891.29166666,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 624708.6681818182,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 8610141.416666668,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 112537930.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 5464668.444444444,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 21641728,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 157866784.43650794,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 8915262.714285713,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 195992.49307904413,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 391.3486997081485,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 193933.13734042554,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1186080232,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 283923335.75,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 383489537.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 167948154.6666667,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 288789.0469906437,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 289890.6655625077,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 332395.85070200916,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 614234.2376811595,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 12049736.669642858,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 23482987,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 47919268.9375,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 258786677.75,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 14238761.57142857,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 233242849.5,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 73096747.625,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 53521894.375,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 219256.34107142856,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 541.4410796630536,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 27.57143,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 189.041235,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 92.5859375,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 2,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 12.98107255520505,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 227.64999999999986,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 31.675813999999995,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 173.700213,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 105.9453125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 23,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 46.04922279792746,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 340.416666666665,
            "unit": "ms",
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
        "date": 1794500459040,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3961202.769230769,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 127418153.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 45755543.19444445,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 85666963.82777777,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 153532842.05555555,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 47689441.84375,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 106203419.6,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 840520.0972222222,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 10975952.899999999,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 129299923.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 7663142.571428571,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 33739538,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 168156364.3125,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 12751435.333333332,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 332203.0541666667,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 544.7701110102325,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 330186.2387012276,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1242664683,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 344761403.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 460151576.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 226096331.5,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 426675.8514492754,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 429424.8787345244,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 485795.49414210126,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 846965.8104779412,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 14241318.732291667,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 25683254.303030305,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 54592544.2,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 304095047.75,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 18375475.3125,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 294782042.75,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 102975844.80000001,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 69588895.36666667,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 352915.1782555224,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 744.1513138145108,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 28.632767,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 268.101448,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 98.890625,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 4,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 19.413793103448278,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 220.0000000000002,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 52.069099,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 236.135739,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 81.82421875,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 34,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 57.462686567164184,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 423.63636363636334,
            "unit": "ms",
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
        "date": 1794501494195,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 2955401.8333333335,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 207890783.1666667,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 42595895.276190475,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 111357459.30555555,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 201757399,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 43206812.47777778,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 113703633.29166666,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 590417.2259615385,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 8165443.214285715,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 105652211.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 5092582.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 20452850,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 158560744.7111111,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 6614152.090909092,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 163295.77344416027,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 387.1224488607569,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 220760.78068181817,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 942795028.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 257602005,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 335435677.75,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 166857932,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 277707.5698457792,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 307042.84659090906,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 354753.4951451451,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 666972.1802983765,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 7734343.583333334,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 14586590.624183007,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 38649154.76041667,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 224282812.5,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 11801158.291666668,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 227068096.5952381,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 75907839.234375,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 54312680.0325,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 195504.2100043728,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 503.6066474564044,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 27.888698,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 142.353356,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 89.28515625,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 2,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 9.985875706214689,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 166.66666666666669,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 27.316443999999997,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 105.121904,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 101.3203125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 21,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 41.8560606060606,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 245.96721311475414,
            "unit": "ms",
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
        "date": 1794503420286,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3167849.625,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 113833730.25,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 37410445.38333333,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 86703827.7875,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 117813951.42857143,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 37708388.375,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 80425360,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 640302.5845078834,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 8736447.333333332,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 113417373,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 5551281.333333334,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 22203787,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 150029177.45,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 9171824.125,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 204674.59692028986,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 424.1474212639703,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 203789.12403961585,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1107402849,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 276230867,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 375053206.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 165827309,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 291501.9894291755,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 295945.73499595816,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 333695.1749898046,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 615052.2208791209,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 13059863.833333332,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 19447025.789473683,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 47513482.39930555,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 260312581.75,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 14160353.055555556,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 263901119.1666667,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 85520059.14583334,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 57772385.71666667,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 227754.31041666667,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 562.463789599899,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 27.714663,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 88.136466,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 88.1796875,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 3,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 15.557553956834534,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 188.70000000000005,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 47.73281,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 280.794764,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 84.74609375,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 24,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 48.24438202247192,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 421.9444444444428,
            "unit": "ms",
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
        "date": 1794517877246,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3503012.2666666666,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 227644077.8333333,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 50315587.266666666,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 136218777.06666666,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 213249782.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 50737956.81071429,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 116255896.50694445,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 696706.7831688595,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 9434490.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 121324479,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 5931213.4375,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 23699775.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 186073562,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 8986457.5,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 188456.38191582,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 456.5306439169098,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 224506.5803840878,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1056924538.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 291630977.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 385163794.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 191584015,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 289019.859375,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 312735.2785,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 385041.35419642855,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 764462.5416666667,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 9293770.708333332,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 15842446.6,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 46870657.08333333,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 259731912.5,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 13587502.9,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 286915970.25,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 87296053.9861111,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 62565263.204166666,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 219696.92242143335,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 585.0662814684265,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 29.408365,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 114.96163299999999,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 93.3671875,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 3,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 16.632096069869,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 208.70000000000005,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 36.28159,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 93.995733,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 78.48828125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 24,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 49.04970760233918,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 393.2142857142854,
            "unit": "ms",
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
        "date": 1794519936005,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3195427.03125,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 104683563.24107143,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 37626562.6875,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 82180747.35,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 113390898.69444445,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 37818107.82142857,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 81687616.08730158,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 622352.5928066187,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 8676956.916666668,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 112822039.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 5589481,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 21975234.25,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 151010137.5,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 10111960.666666666,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 204911.35547367093,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 408.3454115452254,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 200843.76671888054,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1139399325,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 303600274.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 438823799,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 198991651.8333333,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 291532.7106126197,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 293485.0045454545,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 335261.09625771607,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 606539.8006249999,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 10577571.292592593,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 17179880.15625,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 42295400.53888889,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 246395366.75,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 14450552.375,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 297266247.68333334,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 90005395.31111112,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 65451418.24305555,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 227516.24210963456,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 532.8044621761862,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 27.852435,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 233.919879,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 90.8125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 3,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 17.105263157894736,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 252.85714285714434,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 36.769405,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 388.661444,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 78.63671875,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 24,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 47.48677248677249,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 248.4642857142857,
            "unit": "ms",
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
        "date": 1794527233433,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3940097.846153846,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 130269439.05000001,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 45571016.76785714,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 82752730.82142857,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 154451258.96666667,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 45917433.26984127,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 99714933.01190476,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 806433.6168831169,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 11695523.100000001,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 137854727.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 7457371.857142857,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 33530190.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 163241919.8333333,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 12104897.833333332,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 330132.42563657404,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 559.7800559216628,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 326732.89534320205,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1153131895.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 312085983.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 404921602,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 207793729.6666667,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 420979.29114906833,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 421641.85932055744,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 478400.25061813183,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 844308.4165348101,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 13646697.47121212,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 24852926.78125,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 59523989.7,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 297422586.5,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 17967966.9375,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 294827987.3333334,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 99690939.43333334,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 70339207.16666666,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 348804.7191423833,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 742.5282653247405,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 28.940977999999998,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 264.780069,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 91.89453125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 4,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 19.2139175257732,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 223.55263157894717,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 50.283788,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 231.641789,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 78.30078125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 34,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 53.44827586206897,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 424.0909090909084,
            "unit": "ms",
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
        "date": 1794536731576,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3187132.34375,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 110667137.83333334,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 37350109.35714286,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 88318897.85,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 117186704.3125,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 37536848.134920634,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 78021088.6125,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 623270.6814916009,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 8787433.833333332,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 110559228,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 5396405.833333334,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 22330393.25,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 153444073.25,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 11233459.5,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 202783.84190737226,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 412.401406490161,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 193774.5700831195,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1973142853.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 341977126.75,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 433506434.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 174665685.3333333,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 287089.9459325485,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 289535.10466106853,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 331863.32385361555,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 609108.4826190476,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 13955910.66025641,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 19785956.240530305,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 47942145.06666666,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 269396522.75,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 16531761.0625,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 325312925.5333333,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 79586974.8154762,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 56149569.4,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 220839.77155172414,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 557.9333661531982,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 28.078589,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 177.68392699999998,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 89.46875,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 3,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 16.138059701492537,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 209.15909090909085,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 49.529717,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 4749.163136,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 103.703125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 24,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 50.1552795031056,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 457.16666666666583,
            "unit": "ms",
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
        "date": 1794562587386,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3867844.807692308,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 127691780.19444445,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 45808677.15625,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 83248472.55158731,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 156958318.35,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 46043239.66944444,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 100698977.27678572,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 833124.6896853147,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 10764761.1,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 133377512,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 7281488,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 32919403.25,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 168168576.47777778,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 12154252.25,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 331686.8777777778,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 547.5763124058293,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 326991.696125908,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1303382086.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 351253762.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 454556820,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 217045611.5,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 415456.76020408166,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 424230.71600185527,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 480931.11448461446,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 837097.8605964619,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 14240807.111111112,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 22259354.05,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 57785417.58333333,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 299053525.5,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 18326778,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 308313664.8125,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 99573935.61507936,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 68478395.0451389,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 349007.5597883598,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 754.7955082759738,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 28.629667,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 158.23390600000002,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 83.1640625,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 4,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 19.29180887372014,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 215.10526315789505,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 49.84534,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 208.52214,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 106.71875,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 34,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 51.90058479532164,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 443.83333333333326,
            "unit": "ms",
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
        "date": 1794567120759,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3162121.25,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 107358267.94444445,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 38133804.059523806,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 85834868.02777778,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 116089326.28888889,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 38100969.994709,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 79711287.75,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 617026.6610644257,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 8732211.75,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 114295126,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 5430319.444444444,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 22537246,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 151051118.475,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 9651100.785714285,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 200779.75593434344,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 393.33461707333396,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 198247.0551540719,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1373788761,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 321513150.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 394494575.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 194933864.6666667,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 284439.2116813294,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 286457.9307312253,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 331285.6336500236,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 609223.3915700082,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 15209697.146464646,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 21635129,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 54744197.91842105,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 355802266,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 14474455.357142858,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 301510610.6458334,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 82068386.03333333,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 54307276.765625,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 219287.79109337734,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 535.3948811414506,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 28.793781,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 198.11677799999998,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 92.09375,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 2,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 18.80081300813008,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 246.86607142857153,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 79.03113,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 1253.761997,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 97.77734375,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 24,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 46.78571428571429,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 474.7115384615385,
            "unit": "ms",
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
        "date": 1794568223571,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3872842.576923077,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 126918536.75,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 46006422.625,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 89758643.31944445,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 156417958.4166667,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 46475832.88333333,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 101479413.6111111,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 818162.5775162338,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 10910236.2,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 131480021,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 7452725.071428571,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 33527168.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 166154077.25,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 12090577.416666666,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 326214.1666883117,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 526.1186247723133,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 324275.18013784464,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1132884003.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 328067330.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 427446788.75,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 213977906.3333333,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 410224.2882575757,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 416380.5715617168,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 471785.1107142857,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 826138.4721794871,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 14782095.111111112,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 22117912.916666664,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 60231803.5,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 277022859.5,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 17652587.5,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 294759172.7,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 97952109.47222222,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 69320437.6125,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 344945.1948853616,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 736.5129380722674,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 29.166912,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 234.245139,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 92.4375,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 4,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 19.57391304347826,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 237.29411764705836,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 49.657728999999996,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 158.91945199999998,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 81.76953125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 34,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 55.13157894736842,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 394.06250000000045,
            "unit": "ms",
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
        "date": 1794573870302,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3552847.3333333335,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 231702984.3333333,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 50103493.66666667,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 136543580.3875,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 221308534,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 51127999.1875,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 127182961.2125,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 698791.25625,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 9995340.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 124785894.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 6078821.6875,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 24083564,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 190073024.83333334,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 9960005.07142857,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 206541.54591836734,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 457.5080788504564,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 240846.90956072352,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1119756314,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 297662275.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 400698847.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 190762582.6666667,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 293485.4772727273,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 325790.4580238727,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 394389.975,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 754667.6625,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 13535529.65625,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 20186178.291666664,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 49380493.733333334,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 265508766.5,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 14540374.2,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 288403862.65,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 90747209.27083334,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 65288392.77678572,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 227200.95526870392,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 593.7299288160534,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 29.762245999999998,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 210.234861,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 91.94140625,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 3,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 18.51063829787234,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 194.94117647058818,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 41.94853,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 101.364169,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 85.4921875,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 25,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 48.79679144385027,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 376.66666666666515,
            "unit": "ms",
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
        "date": 1794580027880,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3361444.2666666666,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 182753268.28571427,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 52282832.0515873,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 105764307.675,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 186154040.6666667,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 50555232.652777776,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 111363492.80555555,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 756282.0865384615,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 9983234.7,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 119382955,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 6017821.75,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 26799300.25,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 140197800.94047618,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 11336584.357142858,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 277675.6303007519,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 469.58917677142813,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 299754.3357142857,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1127926364,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 330679614.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 419916326.75,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 226333553.8333333,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 442567.39943609026,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 428977.60586080584,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 500357.4375373878,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 877026.7803268385,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 14321192.88095238,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 24019218.32857143,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 60404731.1,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 292821507,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 18494504.5,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 324729788.3833333,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 106029061.1031746,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 70332554.15,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 342990.6302631579,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 615.7789833076567,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 29.95687,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 188.312961,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 93.9609375,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 3,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 19.06465517241379,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 238.09375000000017,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 54.323152,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 215.129706,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 83.85546875,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 30,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 56.515957446808514,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 441.9444444444436,
            "unit": "ms",
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
        "date": 1794594614684,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3862825.6785714286,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 129087205.80555555,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 45982095.0625,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 91279593.95833334,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 154490795.125,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 45905520.625,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 94762566.3968254,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 848983.3565789473,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 10954482.2,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 131385302,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 7348818.142857144,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 32855010,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 167452856.2916667,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 12370125.083333332,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 332546.6950980392,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 533.8911616204477,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 331248.02871305647,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1276392979,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 343151563.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 465778737.75,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 224545185.6666667,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 427074.58702380955,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 427578.1742252456,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 485087.73302107723,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 853205.0845588236,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 13766418.06140351,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 22686979.45,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 55422638.11785714,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 301723151.75,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 18936327.785714284,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 328178490.3833333,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 106125996.93333334,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 73436209.10044643,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 350541.6721681097,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 743.3915902770069,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 28.879502000000002,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 210.128128,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 92.34375,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 4,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 19.40068493150685,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 224.1999999999996,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 50.477299,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 280.39568799999995,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 85.45703125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 34,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 56.34715025906736,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 423.86363636363586,
            "unit": "ms",
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
        "date": 1794596240315,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3151529.90625,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 107577804.42857143,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 37557598.35714285,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 90290144.96666667,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 114629022.125,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 37856865.516666666,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 79947445.26785713,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 623314.2687436678,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 8788816,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 112816609,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 5605092.833333333,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 22697381.25,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 148675699.6111111,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 10264710.4,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 207537.99285714288,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 398.9772800961863,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 208634.77675319643,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1155528046,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 282934094.75,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 385676397,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 170158709.1666667,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 298189.5095128146,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 301489.24133012816,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 342522.04878766474,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 612647.8746087637,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 14503176.888888888,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 18039225.45535714,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 43403769.58119658,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 246379787.5,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 14564857.888888888,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 331112357.325,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 81224968.49131945,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 57679945.11666667,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 231528.85610345536,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 537.3655877211169,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 27.609956,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 112.35235300000001,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 86.17578125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 3,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 14.891304347826088,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 190.80769230769235,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 48.028723,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 735.983379,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 107.49609375,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 24,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 51.865671641791046,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 430.75000000000045,
            "unit": "ms",
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
        "date": 1794601718445,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3180718.71875,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 107301429.85,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 37607721.96296296,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 84669676.33333333,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 112461214.92857143,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 38081587.46666667,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 82076198.71875,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 634252.2097809077,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 9233999.083333332,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 113553956.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 5401140.666666666,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 22501590.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 152418661.55,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 8912638.333333334,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 200065.24166666667,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 392.25403115250333,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 199987.7359094982,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1200126467,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 388382597.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 468812406.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 181781637.6666667,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 290592.3312527186,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 294184.4662698413,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 333684.91622807016,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 635841.504371656,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 22866657.715944275,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 21020013.912593983,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 47928934.64583333,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 285664462.25,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 13708558.5,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 375451786.4166667,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 183931295.5138889,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 74855719.70833334,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 220093.07908163266,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 560.0445696400626,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 27.390284,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 142.372565,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 96.4140625,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 3,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 14.3,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 221.96153846153837,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 49.618239,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 438.142468,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 91.73828125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 24,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 49.18831168831169,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 465.83333333333366,
            "unit": "ms",
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
        "date": 1794603501724,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 2204632.847826087,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 83620052.57142857,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 33051507.074074075,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 75073394.8,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 94435619.3611111,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 33611324.60555555,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 77810224.22321428,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 442482.81738437,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 6140462.333333334,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 92640974,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 4117262.4583333335,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 16352153.666666666,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 127415425.94999999,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 8153305.5,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 146562.48988771642,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 333.9006990467509,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 143595.7523577613,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 945568710.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 235348325.16666666,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 320951998.75,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 141389071.875,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 217939.31865079363,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 209650.81008324382,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 247631.49957482994,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 507902.8574812069,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 9484844.685185187,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 14076290.25877193,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 36283675,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 201060372.25,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 12263066.954545455,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 196055973.375,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 64842217.45,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 46674948.14236111,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 166956.70518451207,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 438.1094104807969,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 28.390553999999998,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 147.730261,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 92.4609375,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 2,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 14.10904255319149,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 134.66666666666734,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 27.098015,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 77.581726,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 76.19140625,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 16,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 33.56164383561644,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 243.84210526315766,
            "unit": "ms",
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
        "date": 1794609392768,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3894649.4615384615,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 135175778.9861111,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 45729365.516666666,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 83287139.06111112,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 154214242.9166667,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 46160846.04444444,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 105051838.03571428,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 802953.9296875,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 10805079.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 131756015.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 7213055.642857143,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 31921681,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 168003276,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 11817666.57142857,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 328920.5804032139,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 545.8612826945453,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 324203.8546975547,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1294128542.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 343182757.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 443572648.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 223455729.3333333,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 421181.7879666546,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 422358.77006392047,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 486940.70905252825,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 895251.6313840654,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 13226968.2,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 23642926.117788464,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 58106249.63392857,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 299443556.5,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 18126525.57142857,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 301577956.2166667,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 101318195.91666666,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 71075419.66666666,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 347321.47535211267,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 766.467285029376,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 27.934649999999998,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 203.39572,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 91.2734375,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 4,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 19.139830508474578,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 232.0750000000001,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 49.717728,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 230.883432,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 81.83203125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 34,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 55.810810810810814,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 406.9444444444438,
            "unit": "ms",
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
        "date": 1794617803686,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 2200439.375,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 85691617.70833333,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 33432519.971428573,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 72568916.175,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 92622840.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 32948471.25,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 82227539.35,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 445377.48890164564,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 5990003.388888889,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 93267683,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 4248389.590909091,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 15845605,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 127270784.97142857,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 8292874.875,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 141499.4877660324,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 331.72729360591563,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 144424.2657563025,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 860021097,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 222039999,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 300549639.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 133772486.5,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 211766.54954228856,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 211593.91796296297,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 246520.9273672257,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 518156.6085316149,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 8821237.122222222,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 12188831.347222222,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 33497629.82222222,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 188923759.5,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 11932648.272727273,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 173842219.1,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 60939729.3525,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 47599025.29166667,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 166458.93486590037,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 430.84174739517726,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 26.674551,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 173.43961199999998,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 83.83203125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 2,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 9.445169712793733,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 191.8999999999994,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 27.626745,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 84.562004,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 75.23828125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 16,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 35.820895522388064,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 239.6285714285716,
            "unit": "ms",
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
        "date": 1794621964003,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3437816.0666666664,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 233660004,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 49112805.33333333,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 132880170.0625,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 222121272.33333334,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 50429055.49107143,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 122042621.28571428,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 667819.3931608788,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 9396246.166666668,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 119672628.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 5790449.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 23519858.75,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 189037638.1,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 8842201.875,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 187108.85471781305,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 444.01865337912295,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 203293.43595848593,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 889662523,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 264813094.75,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 330034786.75,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 180681906.1666667,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 282008.6843377211,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 340723.8964912281,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 406472.60691327724,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 773163.2221943329,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 9769542.837301586,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 18316113.375,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 46771225.91666667,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 248854399.25,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 13583667.100000001,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 274780563.8,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 88159594.44444445,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 64785608.825,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 219654.0117501546,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 570.5099783039689,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 27.936948,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 87.084148,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 86.78515625,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 3,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 13.008982035928145,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 191.38461538461493,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 31.87994,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 144.505663,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 83.39453125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 24,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 48.273480662983424,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 386.2499999999992,
            "unit": "ms",
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
        "date": 1794628389761,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3185258.34375,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 108072334.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 37577002.39814815,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 89382304.99107143,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 116468279.9,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 37705157.03333333,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 75864055.19444445,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 639454.3993453356,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 8795491.083333334,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 115094251.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 5310193.666666666,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 21988388,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 160094451.5,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 16995061.2,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 196890.68656126483,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 386.06833514760865,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 196299.09696969698,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1663984820,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 336386052,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 508341664.75,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 196613715.1666667,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 285866.4461637653,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 292955.06585672393,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 333216.2492776464,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 646664.9677777778,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 19951249.77197802,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 28808755.659090906,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 54399162.80833334,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 292486382,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 18337919.9,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 326549180.1666666,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 94703483.85416666,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 58881423.233333334,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 219557.08349288168,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 548.3212952796985,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 28.64987,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 489.807408,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 84.57421875,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 3,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 17.94117647058824,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 221.71428571428586,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 49.387733000000004,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 171.67703,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 89.92578125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 24,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 48.75,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 263.1250000000023,
            "unit": "ms",
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
        "date": 1794631372790,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3177500.625,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 106381527.475,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 37419612.14583333,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 87198632.67222223,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 114673374.03968254,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 37967928.375,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 82887404.09722222,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 628369.7005012531,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 8598000.916666668,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 111170713,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 5640202.666666666,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 22312438.166666664,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 157966834.51666665,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 8772416.875,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 201719.3628458498,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 374.7490340821132,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 197571.91470795212,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1038927991,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 273842891.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 360176022,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 165002011.1666667,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 285892.8032961191,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 289273.70904040406,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 328363.9038812786,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 649381.987592789,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 9613868.281818181,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 15813691.107142858,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 40123023.38888889,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 238558116.75,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 12705123.2,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 210487633.4166667,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 74431263.828125,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 54900004.57619047,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 218216.05245393177,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 550.0036561849668,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 26.738367999999998,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 185.392168,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 88.78515625,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 2,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 10.597826086956522,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 178.9230769230773,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 31.441174000000004,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 187.519772,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 84.2109375,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 23,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 48.48901098901099,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 248.27941176470594,
            "unit": "ms",
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
        "date": 1794638143818,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3932466.538461538,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 129486372.85714287,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 45807003.69791667,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 88596673.98333333,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 153235021.1458333,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 46339566,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 108948943.1875,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 841935.8730701755,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 11410057.600000001,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 131688495.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 7057371.571428571,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 33061845,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 169890375.5,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 11794141.083333332,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 333177.40023320896,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 532.9007951882807,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 327677.02521008404,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1242039012.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 329601269.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 421070811.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 216131208.33333334,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 412714.43118982564,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 423713.0436607143,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 490783.14285714284,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 894405.7887345679,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 13873432.319444444,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 23904112.50986842,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 56662250.5,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 295410118.75,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 17947786.6875,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 293808521,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 101574194.8888889,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 72632960.6111111,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 350975.15523845155,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 750.0540471184488,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 28.41821,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 180.798188,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 89.13671875,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 4,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 19.35553633217993,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 207.92105263157862,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 49.109775,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 266.908615,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 89.3046875,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 34,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 55.42328042328043,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 393.437500000001,
            "unit": "ms",
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
        "date": 1794643458645,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 2148554.9782608696,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 87962064.04166666,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 33258482.866666667,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 74820225.21825397,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 94606048.83333333,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 33439091.015873015,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 78046935.125,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 458276.782051282,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 7234203.357142857,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 98466401.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 3943769.5416666665,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 16660257.166666666,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 117274466,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 9233562.8125,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 138430.16514945653,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 402.197410472981,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 143177.89235929147,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 881854820.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 212885937.1666667,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 309757864,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 135257611,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 216160.34166666667,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 215539.87264325586,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 251052.35594315245,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 506801.5721153846,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 9364143.104166668,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 14004444.701704545,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 35698116.24848485,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 199723626,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 12195840.363636363,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 171518249.85,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 64990679.91428571,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 46636410.13194445,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 169009.0439133987,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 603.810071879789,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 28.269078,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 159.666832,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 86.65234375,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 2,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 12.612612612612612,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 135.0000000000004,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 27.573013,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 85.91048799999999,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 77.61328125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 17,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 35.583333333333336,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 239.3285714285714,
            "unit": "ms",
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
        "date": 1794645024859,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "inprocess/decode/fixture",
            "value": 3873720.346153846,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/1",
            "value": 130679649.25,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/256",
            "value": 45978485.625,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks1/64",
            "value": 89618349.125,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/1",
            "value": 156160931.8333333,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/256",
            "value": 46252233.44166666,
            "unit": "ns"
          },
          {
            "name": "inprocess/live_drain/sinks2/64",
            "value": 116227905.04166666,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/delete",
            "value": 824771.8217189314,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/mid",
            "value": 11623506,
            "unit": "ns"
          },
          {
            "name": "inprocess/render/wide",
            "value": 138610510.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/flat",
            "value": 7301602.785714285,
            "unit": "ns"
          },
          {
            "name": "inprocess/scripted/nested",
            "value": 33461487.5,
            "unit": "ns"
          },
          {
            "name": "inprocess/snapshot/rows",
            "value": 167154922.8333333,
            "unit": "ns"
          },
          {
            "name": "components/baseline/os_flush_1",
            "value": 12308843.5,
            "unit": "ns"
          },
          {
            "name": "components/baseline/pg_select_1",
            "value": 332391.23122849624,
            "unit": "ns"
          },
          {
            "name": "components/baseline/resolve_unrelated",
            "value": 693.8333247444334,
            "unit": "ns"
          },
          {
            "name": "components/baseline/select_1",
            "value": 328817.63650392817,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/100",
            "value": 1279838163,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/1000",
            "value": 345281462.25,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/500",
            "value": 461989558.5,
            "unit": "ns"
          },
          {
            "name": "components/batch_size/5000",
            "value": 225643880.33333334,
            "unit": "ns"
          },
          {
            "name": "components/build/0",
            "value": 428180.4030812937,
            "unit": "ns"
          },
          {
            "name": "components/build/1",
            "value": 425980.01585299947,
            "unit": "ns"
          },
          {
            "name": "components/build/10",
            "value": 491073.2143248747,
            "unit": "ns"
          },
          {
            "name": "components/build/100",
            "value": 965430.266025641,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1",
            "value": 15602222.291666668,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/100",
            "value": 25778184.45378151,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/1000",
            "value": 59558460.1,
            "unit": "ns"
          },
          {
            "name": "components/bulk_index/5000",
            "value": 314490537.25,
            "unit": "ns"
          },
          {
            "name": "components/change/item_update",
            "value": 18718249.285714284,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/1",
            "value": 307773579.06666666,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/16",
            "value": 102246848.94444445,
            "unit": "ns"
          },
          {
            "name": "components/change_burst/256",
            "value": 72394718.90104166,
            "unit": "ns"
          },
          {
            "name": "components/resolve/related_table",
            "value": 355243.66500771604,
            "unit": "ns"
          },
          {
            "name": "components/resolve/root_table",
            "value": 1140.0874394375269,
            "unit": "ns"
          },
          {
            "name": "complex/ci/visible_latency_p50_ms",
            "value": 29.081032999999998,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/visible_latency_p99_ms",
            "value": 240.01904399999998,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/peak_rss_mib",
            "value": 88.6875,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/cpu_seconds",
            "value": 4,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p50_ms",
            "value": 19.484536082474225,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "complex/ci/flush_p99_ms",
            "value": 238.89999999999986,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":4000,\"items_per_order\":4,\"name\":\"ci\",\"orders\":0,\"orders_per_user\":5,\"probe_rate_per_s\":20,\"probes\":150,\"products\":0,\"reviews_per_product\":0,\"rss_cap_mib\":2048,\"tags\":8,\"tags_per_user\":4,\"users\":2000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p50_ms",
            "value": 49.632951,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/visible_latency_p99_ms",
            "value": 237.074088,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/peak_rss_mib",
            "value": 76.86328125,
            "unit": "MiB",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/cpu_seconds",
            "value": 36,
            "unit": "s",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p50_ms",
            "value": 58.5972850678733,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          },
          {
            "name": "reference/ci/flush_p99_ms",
            "value": 420.0000000000004,
            "unit": "ms",
            "extra": "{\"images\":{\"opensearch\":\"opensearchproject/opensearch:2\",\"postgres\":\"postgres:16-alpine\"},\"scale\":{\"burst\":10000,\"items_per_order\":3,\"name\":\"ci\",\"orders\":25000,\"orders_per_user\":0,\"probe_rate_per_s\":20,\"probes\":200,\"products\":1000,\"reviews_per_product\":2,\"rss_cap_mib\":2048,\"tags\":0,\"tags_per_user\":0,\"users\":5000,\"wall_cap_secs\":900,\"writers\":8}}"
          }
        ]
      }
    ]
  }
}