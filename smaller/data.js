window.BENCHMARK_DATA = {
  "lastUpdate": 1788703252870,
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
      }
    ]
  }
}