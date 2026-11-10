window.BENCHMARK_DATA = {
  "lastUpdate": 1794327175771,
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
      }
    ]
  }
}