const $ = (id) => document.getElementById(id);
const metricNames = { wer: 'word error rate', cer: 'character error rate', exact_match_rate: 'exact match', latency_median_s: 'median latency' };
const filterKeys = ['task', 'dataset', 'split'];
let runs = [];
let selectedId = null;

function element(tag, text, className) {
  const node = document.createElement(tag);
  if (text !== undefined) node.textContent = text;
  if (className) node.className = className;
  return node;
}
function format(value, metric) {
  if (value === null || value === undefined) return 'Unavailable';
  return metric === 'latency_median_s' ? `${value.toFixed(2)} s` : `${(value * 100).toFixed(1)}%`;
}
function validate(data) {
  if (data?.schema_version !== 1 || !Array.isArray(data.runs)) throw new Error('Unsupported results schema. Expected schema_version 1 and a runs array.');
  const ids = new Set();
  for (const run of data.runs) {
    for (const key of ['id', 'label', 'task', 'dataset', 'split', 'model', 'reference_status']) {
      if (typeof run[key] !== 'string' || !run[key].trim()) throw new Error(`A run has a missing or invalid ${key}.`);
    }
    if (ids.has(run.id)) throw new Error('Duplicate run IDs in results.');
    ids.add(run.id);
    if (!Number.isInteger(run.samples) || run.samples < 0) throw new Error('Invalid sample count.');
    for (const metric of Object.keys(metricNames)) {
      const value = run[metric];
      if (value !== null && (typeof value !== 'number' || !Number.isFinite(value) || value < 0)) throw new Error(`Invalid ${metric} measurement.`);
      if (metric === 'exact_match_rate' && value > 1) throw new Error('Invalid exact match rate.');
    }
    if (run.prompt_id !== null && typeof run.prompt_id !== 'string') throw new Error('Invalid prompt ID.');
    if (!Array.isArray(run.limitations) || run.limitations.some((item) => typeof item !== 'string')) throw new Error('Invalid run limitations.');
    if (!run.configuration || typeof run.configuration !== 'object' || Array.isArray(run.configuration)) throw new Error('Invalid run configuration.');
    if (run.examples !== undefined && (!Array.isArray(run.examples) || run.examples.some((example) => !example || ['input', 'reference', 'output'].some((key) => typeof example[key] !== 'string' && example[key] !== null)))) throw new Error('Invalid public examples.');
  }
  return data.runs;
}
function clearDetails() {
  selectedId = null;
  $('detail').replaceChildren(element('h2', 'Run evidence'), element('p', 'Select a run from the chart or table to inspect its prompt, configuration, reference status, and limitations.', 'muted'));
  $('detail').firstChild.id = 'detail-title';
  $('detail').firstChild.tabIndex = -1;
}
function showRun(run) {
  selectedId = run.id;
  const heading = element('h2', run.label);
  heading.id = 'detail-title';
  heading.tabIndex = -1;
  const meta = element('dl');
  for (const [name, value] of [['Run ID', run.id], ['Model', run.model], ['Task', run.task], ['Dataset / split', `${run.dataset} / ${run.split}`], ['Samples', run.samples.toLocaleString()], ['Prompt ID', run.prompt_id || 'Not applicable or unavailable'], ['Reference status', run.reference_status]]) {
    meta.append(element('dt', name), element('dd', value));
  }
  const list = element('ul');
  if (run.limitations.length) run.limitations.forEach((item) => list.append(element('li', item)));
  else list.append(element('li', 'No run-specific limitations were supplied. This does not establish general performance.'));
  const configuration = element('details');
  configuration.append(element('summary', 'Prompt and configuration'), element('pre', JSON.stringify(run.configuration, null, 2)));
  const examples = element('details');
  examples.append(element('summary', `Public examples (${run.examples?.length || 0})`));
  if (run.examples?.length) {
    run.examples.forEach((example, index) => {
      const block = element('article', undefined, 'example');
      block.append(element('h3', `Example ${index + 1}`));
      for (const [label, key] of [['Input', 'input'], ['Reference', 'reference'], ['Output', 'output']]) block.append(element('h3', label), element('p', example[key] ?? 'Unavailable'));
      examples.append(block);
    });
  } else examples.append(element('p', 'No public text examples were exported for this run.', 'muted'));
  $('detail').replaceChildren(heading, meta, element('h3', 'Limitations'), list, configuration, examples);
  document.querySelectorAll('[data-run-id]').forEach((node) => {
    const selected = node.dataset.runId === run.id;
    if (node.tagName === 'BUTTON') node.setAttribute('aria-pressed', String(selected));
    else node.dataset.selected = String(selected);
  });
  heading.focus({ preventScroll: true });
  $('detail').scrollIntoView({ behavior: 'instant', block: 'start' });
}
function render() {
  const metric = $('metric').value;
  const filtered = runs.filter((run) => filterKeys.every((key) => !$(key).value || run[key] === $(key).value));
  filtered.sort((a, b) => {
    if (a[metric] === null && b[metric] !== null) return 1;
    if (b[metric] === null && a[metric] !== null) return -1;
    return (metric === 'exact_match_rate' ? -1 : 1) * ((a[metric] ?? 0) - (b[metric] ?? 0)) || a.label.localeCompare(b.label);
  });
  if (!filtered.some((run) => run.id === selectedId)) clearDetails();
  $('count').textContent = `${filtered.length} of ${runs.length} runs`;
  $('metric-note').textContent = `${metric === 'exact_match_rate' ? 'Higher' : 'Lower'} ${metricNames[metric]} is better. Missing measurements are unavailable.`;
  $('status').textContent = !runs.length ? 'No public runs have been published yet. Add a public benchmark export to results.json.' : !filtered.length ? 'No runs match these filters. Reset the filters to see all public runs.' : '';
  $('rows').replaceChildren();
  $('chart').replaceChildren();
  const max = Math.max(...filtered.map((run) => run[metric] ?? 0), 0.001);
  for (const run of filtered) {
    const bar = element('button', undefined, 'chart-row');
    bar.type = 'button';
    bar.dataset.runId = run.id;
    bar.setAttribute('aria-pressed', String(run.id === selectedId));
    bar.setAttribute('aria-label', `${run.label}, ${metricNames[metric]} ${format(run[metric], metric)}. Inspect run.`);
    const track = element('span', undefined, 'chart-track');
    track.setAttribute('aria-hidden', 'true');
    const fill = element('span', undefined, 'chart-fill');
    fill.style.width = `${(run[metric] ?? 0) / max * 100}%`;
    track.append(fill);
    bar.append(element('span', run.label, 'chart-name'), track, element('span', run[metric] === null ? 'N/A' : format(run[metric], metric), 'chart-value'));
    bar.addEventListener('click', () => showRun(run));
    $('chart').append(bar);
    const row = element('tr');
    row.dataset.runId = run.id;
    row.dataset.selected = String(run.id === selectedId);
    const name = element('td');
    const button = element('button', run.label);
    button.type = 'button';
    button.addEventListener('click', () => showRun(run));
    name.append(button, element('small', run.model));
    const context = element('td');
    context.append(element('span', run.task), element('small', `${run.dataset} / ${run.split}`));
    row.append(name, context, element('td', run.samples.toLocaleString()));
    for (const key of Object.keys(metricNames)) row.append(element('td', format(run[key], key)));
    row.append(element('td', run.reference_status));
    $('rows').append(row);
  }
}
function renderOptimization(experiments) {
  const section = $('optimization');
  const target = $('optimization-results');
  target.replaceChildren();
  section.hidden = !Array.isArray(experiments) || !experiments.length;
  if (section.hidden) return;
  const table = element('table');
  const head = element('tr');
  for (const title of ['Experiment', 'Initial WER', 'Best WER', 'Candidates', 'Accepted']) { const cell = element('th', title); cell.scope = 'col'; head.append(cell); }
  const thead = element('thead'); thead.append(head); table.append(thead);
  const body = element('tbody');
  for (const experiment of experiments) {
    if (typeof experiment.id !== 'string' || ['initial_wer', 'best_wer', 'candidates', 'accepted'].some((key) => typeof experiment[key] !== 'number' || !Number.isFinite(experiment[key]) || experiment[key] < 0)) throw new Error('Invalid optimization summary.');
    const row = element('tr');
    for (const value of [experiment.id, format(experiment.initial_wer, 'wer'), format(experiment.best_wer, 'wer'), String(experiment.candidates), String(experiment.accepted)]) row.append(element('td', value));
    body.append(row);
  }
  table.append(body); const scroll = element('div', undefined, 'table-scroll'); scroll.tabIndex = 0; scroll.setAttribute('role', 'region'); scroll.setAttribute('aria-label', 'Scrollable optimization results'); scroll.append(table); target.append(scroll);
}
async function load() {
  $('retry').hidden = true;
  $('status').textContent = 'Loading public benchmark results…';
  try {
    const response = await fetch('./results.json', { cache: 'no-store' });
    if (!response.ok) throw new Error(`Results request failed with HTTP ${response.status}.`);
    const payload = await response.json();
    runs = validate(payload);
    renderOptimization(payload.optimization);
    for (const key of filterKeys) {
      const previous = $(key).value;
      $(key).replaceChildren(new Option(`All ${key === 'task' ? 'tasks' : key === 'dataset' ? 'datasets' : 'splits'}`, ''));
      [...new Set(runs.map((run) => run[key]))].sort().forEach((value) => $(key).add(new Option(value, value)));
      if ([...$(key).options].some((option) => option.value === previous)) $(key).value = previous;
    }
    render();
  } catch (error) {
    runs = [];
    $('optimization').hidden = true;
    $('rows').replaceChildren();
    $('chart').replaceChildren();
    clearDetails();
    $('count').textContent = 'Results unavailable';
    $('status').textContent = `Could not load results. ${error.message} Retry, or inspect the public JSON download.`;
    $('retry').hidden = false;
  }
}
for (const key of [...filterKeys, 'metric']) $(key).addEventListener('change', render);
$('reset').addEventListener('click', () => { filterKeys.forEach((key) => { $(key).value = ''; }); render(); });
$('retry').addEventListener('click', load);
let theme = 'dark';
try { const saved = localStorage.getItem('dipper-bench-theme'); if (['system', 'dark', 'light'].includes(saved)) theme = saved; } catch { /* Storage may be disabled. Theme still works for this visit. */ }
function setTheme() {
  if (theme === 'system') delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = theme;
  const next = { system: 'dark', dark: 'light', light: 'system' }[theme];
  $('theme').textContent = `Theme: ${theme}`;
  $('theme').setAttribute('aria-label', `Color theme: ${theme}. Switch to ${next}.`);
}
setTheme();
$('theme').addEventListener('click', () => {
  theme = { system: 'dark', dark: 'light', light: 'system' }[theme];
  setTheme();
  try { localStorage.setItem('dipper-bench-theme', theme); } catch { /* Keep the theme in memory when storage is unavailable. */ }
});
load();
