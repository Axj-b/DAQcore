// DAQcore landing page — live mock telemetry charts + interactivity.

function setupChart(canvas, series) {
  const ctx = canvas.getContext('2d');
  const dpr = window.devicePixelRatio || 1;
  const width = canvas.clientWidth || canvas.width;
  const height = canvas.clientHeight || canvas.height;
  canvas.width = width * dpr;
  canvas.height = height * dpr;
  ctx.scale(dpr, dpr);

  const data = series.map(() => []);
  const N = Math.floor(width / 2);
  const colors = series.map((s) => s.color);

  function push(s, i, v) {
    data[i].push(v);
    if (data[i].length > N) data[i].shift();
  }

  // seed
  for (let i = 0; i < N; i++) {
    series.forEach((s, idx) => {
      push(s, idx, s.base + Math.random() * s.noise);
    });
  }

  function draw() {
    ctx.clearRect(0, 0, width, height);
    ctx.fillStyle = '#0c1526';
    ctx.fillRect(0, 0, width, height);

    // grid
    ctx.strokeStyle = 'rgba(29,42,66,0.5)';
    ctx.lineWidth = 1;
    for (let y = 0; y <= 4; y++) {
      ctx.beginPath();
      ctx.moveTo(0, (height / 4) * y);
      ctx.lineTo(width, (height / 4) * y);
      ctx.stroke();
    }

    series.forEach((s, idx) => {
      const min = s.min;
      const max = s.max;
      const pts = data[idx];
      ctx.strokeStyle = s.color;
      ctx.lineWidth = 2;
      ctx.beginPath();
      pts.forEach((v, x) => {
        const px = (x / N) * width;
        const py = height - ((v - min) / (max - min)) * height;
        if (x === 0) ctx.moveTo(px, py);
        else ctx.lineTo(px, py);
      });
      ctx.stroke();
    });
  }

  function tick() {
    series.forEach((s, idx) => {
      s.phase = (s.phase || 0) + s.speed;
      const wave = Math.sin(s.phase) * s.amp;
      const drift = Math.random() * s.noise - s.noise / 2;
      let v = s.base + wave + drift;
      v = Math.max(s.min, Math.min(s.max, v));
      push(s, idx, v);
    });
    draw();
  }

  setInterval(tick, 40);
  draw();
  return { canvas };
}

// Hero mini chart
const heroSeries = [
  { base: 85, amp: 2.2, noise: 1.4, min: 70, max: 95, speed: 0.05, color: '#22d3ee' },   // temp
  { base: 24, amp: 0.4, noise: 0.5, min: 20, max: 28, speed: 0.04, color: '#34d399' },   // voltage
  { base: 3.2, amp: 1.5, noise: 0.6, min: 0, max: 6, speed: 0.06, color: '#a78bfa' },    // current
];
setupChart(document.getElementById('chart'), heroSeries);

// Live demo chart
const liveSeries = [
  { base: 85, amp: 4, noise: 3, min: 60, max: 105, speed: 0.03, color: '#22d3ee' },
  { base: 24, amp: 1, noise: 1, min: 18, max: 30, speed: 0.025, color: '#34d399' },
  { base: 3.2, amp: 3, noise: 1.4, min: 0, max: 7, speed: 0.035, color: '#a78bfa' },
];
setupChart(document.getElementById('chart2'), liveSeries);

// Live metric readouts (hero)
const temp = document.getElementById('m-temp');
const volt = document.getElementById('m-volt');
const amp = document.getElementById('m-amp');
setInterval(() => {
  const t = (85 + Math.sin(Date.now() / 1200) * 2 + (Math.random() - 0.5)).toFixed(1);
  const v = (24 + Math.sin(Date.now() / 1600) * 0.4 + (Math.random() - 0.5) * 0.3).toFixed(2);
  const a = (3.2 + Math.sin(Date.now() / 900) * 1.5 + (Math.random() - 0.5) * 0.5).toFixed(3);
  temp.textContent = t + ' °C';
  volt.textContent = v + ' V';
  amp.textContent = a + ' A';
}, 500);

// Copy command button
const copyBtn = document.getElementById('copy');
copyBtn.addEventListener('click', () => {
  const cmd = 'curl -sSL https://daqcore.dev/install | sh';
  navigator.clipboard.writeText(cmd).then(() => {
    copyBtn.textContent = 'Copied ✓';
    setTimeout(() => (copyBtn.textContent = 'Copy'), 1600);
  });
});
