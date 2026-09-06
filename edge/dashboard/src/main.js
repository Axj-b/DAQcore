import uPlot from 'uplot';
import 'uplot/dist/uPlot.min.css';

const ACCENT = '#44cee8';
const MAX_POINTS = 200_000;
const RECONNECT_MS = 1000;

const $ = (id) => document.getElementById(id);
const statusEl = $('status');
const metricsEl = $('metrics');
const chartsEl = $('charts');

/** channel id -> { plot, x:number[], y:number[], pending:number[] } */
const channels = new Map();
let devices = [];
let dirty = false;

function setStatus(text, ok = true) {
  statusEl.textContent = text;
  statusEl.className = 'status ' + (ok ? 'ok' : 'err');
}

/* ---------------- tabs ---------------- */

function initTabs() {
  $('tabs').addEventListener('click', (e) => {
    const btn = e.target.closest('button[data-tab]');
    if (!btn) return;
    const tab = btn.dataset.tab;
    for (const b of document.querySelectorAll('#tabs button')) {
      b.classList.toggle('active', b === btn);
    }
    for (const v of document.querySelectorAll('.view')) {
      v.classList.toggle('active', v.id === 'view-' + tab);
    }
    if (tab === 'live') onResize();
  });
}

/* ---------------- live charts ---------------- */

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

function ensureChart(id, unit) {
  if (!channels.has(id)) channels.set(id, makeChart(id, unit || ''));
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

/* ---------------- devices ---------------- */

async function refreshDevices() {
  const res = await fetch('/api/v1/devices');
  const data = await res.json();
  devices = data.devices || [];
  for (const d of devices) {
    for (const c of d.channels || []) ensureChart(c.id, c.unit);
  }
  renderDeviceTable();
  renderIoSelect();
}

function renderDeviceTable() {
  const tbody = document.querySelector('#device-table tbody');
  tbody.innerHTML = '';
  for (const d of devices) {
    const tr = document.createElement('tr');
    tr.innerHTML =
      `<td>${escapeHtml(d.id)}</td>` +
      `<td>${escapeHtml(d.kind)}</td>` +
      `<td class="${d.connected ? 'ok-text' : 'err-text'}">${d.connected ? 'connected' : 'offline'}</td>` +
      `<td>${escapeHtml((d.channels || []).map((c) => c.id).join(', '))}</td>`;
    tbody.appendChild(tr);
  }
}

function renderIoSelect() {
  const sel = $('io-device');
  const current = sel.value;
  sel.innerHTML = '';
  for (const d of devices) {
    const opt = document.createElement('option');
    opt.value = d.id;
    opt.textContent = d.id;
    sel.appendChild(opt);
  }
  if (current) sel.value = current;
}

async function ioSample() {
  const id = $('io-device').value;
  const out = $('io-result');
  if (!id) return;
  out.textContent = '…';
  try {
    const res = await fetch(`/api/v1/devices/${id}/sample`, { method: 'POST' });
    const data = await res.json();
    out.textContent = JSON.stringify(data, null, 2);
  } catch (e) {
    out.textContent = 'error: ' + e;
  }
}

async function ioCommand() {
  const id = $('io-device').value;
  const op = $('io-op').value.trim();
  const out = $('io-result');
  if (!id || !op) return;
  out.textContent = '…';
  try {
    const res = await fetch(`/api/v1/devices/${id}/command`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ op }),
    });
    const data = await res.json();
    out.textContent = JSON.stringify(data, null, 2);
  } catch (e) {
    out.textContent = 'error: ' + e;
  }
}

async function addDevice() {
  const out = $('add-device-result');
  const raw = $('add-device-json').value.trim();
  if (!raw) return;
  out.textContent = '…';
  try {
    const body = JSON.parse(raw);
    const res = await fetch('/api/v1/devices', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    const data = await res.json();
    if (!res.ok) {
      out.textContent = 'error: ' + JSON.stringify(data);
      return;
    }
    out.textContent = 'added: ' + JSON.stringify(data, null, 2);
    $('add-device-json').value = '';
    refreshDevices();
  } catch (e) {
    out.textContent = 'error: ' + e;
  }
}

/* ---------------- system ---------------- */

async function pollSystem() {
  try {
    const s = await fetch('/api/v1/system').then((r) => r.json());
    const cpu = s.cpu_percent;
    const memPct = s.mem_total_bytes ? (s.mem_used_bytes / s.mem_total_bytes) * 100 : 0;
    $('cpu-bar').style.width = Math.min(100, cpu) + '%';
    $('cpu-val').textContent = cpu.toFixed(1) + ' %';
    $('mem-bar').style.width = Math.min(100, memPct) + '%';
    $('mem-val').textContent = fmtBytes(s.mem_used_bytes) + ' / ' + fmtBytes(s.mem_total_bytes);
  } catch {
    /* server not up yet */
  }
}

function fmtBytes(n) {
  if (n >= 1 << 30) return (n / (1 << 30)).toFixed(1) + ' GB';
  if (n >= 1 << 20) return (n / (1 << 20)).toFixed(0) + ' MB';
  return n + ' B';
}

/* ---------------- metrics / health ---------------- */

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
    /* not up yet */
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

function escapeHtml(s) {
  return String(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
}

/* ---------------- websocket ---------------- */

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

/* ---------------- init ---------------- */

function bind() {
  $('io-sample').addEventListener('click', ioSample);
  $('io-send').addEventListener('click', ioCommand);
  $('add-device-btn').addEventListener('click', addDevice);
  window.addEventListener('resize', onResize);
}

async function main() {
  initTabs();
  bind();
  await refreshDevices();
  if (channels.size === 0) {
    chartsEl.innerHTML = '<p class="hint">No channels configured.</p>';
  }
  connect();
  pollMetrics();
  pollSystem();
  setInterval(pollMetrics, 1000);
  setInterval(pollSystem, 1000);
  requestAnimationFrame(rafLoop);
}

main();
