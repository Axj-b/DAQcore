import uPlot from 'uplot';
import 'uplot/dist/uPlot.min.css';

const ACCENT = '#44cee8';
// Cap per-channel memory so the UI stays snappy even after hours of streaming.
const MAX_POINTS = 200_000;
const RECONNECT_MS = 1000;

const statusEl = document.getElementById('status');
const metricsEl = document.getElementById('metrics');
const chartsEl = document.getElementById('charts');

/** channel id -> { plot, x:number[], y:number[], pending:number[] } */
const channels = new Map();
let dirty = false;

function setStatus(text, ok = true) {
  statusEl.textContent = text;
  statusEl.className = 'status ' + (ok ? 'ok' : 'err');
}

function makeChart(id, unit) {
  const wrapper = document.createElement('div');
  wrapper.className = 'chart';

  const head = document.createElement('div');
  head.className = 'chart-head';
  head.textContent = unit ? `${id} · ${unit}` : id;
  wrapper.appendChild(head);

  const el = document.createElement('div');
  el.className = 'chart-body';
  wrapper.appendChild(el);
  chartsEl.appendChild(wrapper);

  const opts = {
    width: chartsEl.clientWidth || 800,
    height: 180,
    scales: { x: { time: true }, y: {} },
    series: [{}, { label: id, stroke: ACCENT, width: 1.5, spanGaps: false }],
    axes: [
      { stroke: '#3a3f4b', grid: { stroke: '#1c2129' }, ticks: { stroke: '#3a3f4b' } },
      { stroke: '#3a3f4b', grid: { stroke: '#1c2129' }, size: 70 },
    ],
    cursor: { drag: { x: true, y: false } },
    legend: { show: false },
  };

  const plot = new uPlot(opts, [[], []], el);
  return { plot, x: [], y: [], pending: [] };
}

async function loadDrivers() {
  const res = await fetch('/api/v1/drivers');
  const data = await res.json();
  for (const d of data.drivers || []) {
    for (const c of d.channels || []) {
      if (channels.has(c.id)) continue;
      channels.set(c.id, makeChart(c.id, c.unit || ''));
    }
  }
}

async function pollMetrics() {
  try {
    const [m, h] = await Promise.all([
      fetch('/api/v1/metrics').then((r) => r.json()),
      fetch('/api/v1/health').then((r) => r.json()),
    ]);
    metricsEl.innerHTML =
      `uptime <b>${fmtDuration(h.uptime_secs)}</b> · ` +
      `written <b>${m.written_seq}</b> · acked <b>${m.acknowledged_seq}</b> · ` +
      `buffered <b>${m.buffered}</b> · samples <b>${fmtCount(m.total_samples)}</b>`;
  } catch {
    /* server not up yet */
  }
}

function fmtDuration(s) {
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = Math.floor(s % 60);
  return `${h}h ${m}m ${sec}s`;
}

function fmtCount(n) {
  if (n >= 1e6) return (n / 1e6).toFixed(1) + 'M';
  if (n >= 1e3) return (n / 1e3).toFixed(1) + 'k';
  return String(n);
}

function decimate(ch) {
  if (ch.x.length <= MAX_POINTS) return;
  const n = ch.x.length >> 1;
  const nx = new Array(n);
  const ny = new Array(n);
  for (let i = 0, j = 0; i < ch.x.length; i += 2, j++) {
    nx[j] = ch.x[i];
    ny[j] = ch.y[i];
  }
  ch.x = nx;
  ch.y = ny;
}

function flushPending() {
  for (const ch of channels.values()) {
    if (ch.pending.length === 0) continue;
    for (let i = 0; i < ch.pending.length; i += 2) {
      ch.x.push(ch.pending[i]);
      ch.y.push(ch.pending[i + 1]);
    }
    ch.pending.length = 0;
    decimate(ch);
    ch.plot.setData([ch.x, ch.y]);
  }
}

function rafLoop() {
  if (dirty) {
    dirty = false;
    flushPending();
  }
  requestAnimationFrame(rafLoop);
}

function connect() {
  const proto = location.protocol === 'https:' ? 'wss' : 'ws';
  const ws = new WebSocket(`${proto}://${location.host}/api/v1/live`);
  ws.onopen = () => setStatus('live');
  ws.onmessage = (e) => {
    let batch;
    try {
      batch = JSON.parse(e.data);
    } catch {
      return;
    }
    for (const s of batch.samples) {
      const ch = channels.get(s.channel);
      if (!ch) continue;
      ch.pending.push(s.ts_ms, s.value);
    }
    if (batch.samples.length > 0) dirty = true;
  };
  ws.onclose = () => {
    setStatus('reconnecting…', false);
    setTimeout(connect, RECONNECT_MS);
  };
  ws.onerror = () => ws.close();
}

function onResize() {
  const width = chartsEl.clientWidth || 800;
  for (const ch of channels.values()) {
    ch.plot.setSize({ width, height: 180 });
  }
}

async function main() {
  await loadDrivers();
  if (channels.size === 0) {
    chartsEl.innerHTML = '<p class="empty">No channels configured.</p>';
  }
  connect();
  pollMetrics();
  setInterval(pollMetrics, 1000);
  window.addEventListener('resize', onResize);
  requestAnimationFrame(rafLoop);
}

main();
