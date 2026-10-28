# Dashboard assets

Static assets served by `cosw dashboard` (see `cosw/dashboard.py`).

- `dashboard.html` — the single-page dashboard (adapted from
  [watson-dashboard](https://github.com/chr1st1ank/watson-dashboard)).
- `chart.umd.min.js` — vendored [Chart.js](https://www.chartjs.org) so the
  dashboard works offline. Pinned version: **4.5.1** (MIT). To update,
  download `https://cdn.jsdelivr.net/npm/chart.js@<version>/dist/chart.umd.min.js`
  and bump the version noted here.
- `fonts/` — the [Carlito](https://github.com/googlefonts/carlito) family
  (SIL Open Font License, see `fonts/OFL.txt`), metric-compatible with
  Calibri. Embedded by `cosw export` for professional PDF timesheets with
  full Unicode coverage. To update, download the TTFs from
  `ofl/carlito/` in <https://github.com/google/fonts>.
