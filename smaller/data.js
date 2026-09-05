window.BENCHMARK_DATA = {
  "lastUpdate": 1788626129377,
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
      }
    ]
  }
}