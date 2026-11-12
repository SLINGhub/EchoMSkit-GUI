const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const vm = require("node:vm");

const uiDirectory = path.join(__dirname, "../ui");
const html = fs.readFileSync(path.join(uiDirectory, "index.html"), "utf8");
const source = fs.readFileSync(path.join(uiDirectory, "app.js"), "utf8");

function element() {
  const classes = new Set();
  return {
    value: "", valueAsNumber: 0, textContent: "", disabled: false, style: {}, options: [],
    classList: {
      add: (...names) => names.forEach((name) => classes.add(name)),
      remove: (...names) => names.forEach((name) => classes.delete(name)),
      contains: (name) => classes.has(name),
      toggle(name, force) {
        const enabled = force ?? !classes.has(name);
        if (enabled) classes.add(name); else classes.delete(name);
        return enabled;
      },
    },
    replaceChildren(...children) { this.options = children; },
    append() {},
    querySelector: () => element(),
    setAttribute() {},
  };
}

function loadApp(invoke = async () => {}) {
  const nodes = new Map();
  const context = vm.createContext({
    window: {
      __TAURI__: { core: { invoke } },
      requestAnimationFrame() {}, clearTimeout() {}, setTimeout() {},
    },
    document: {
      querySelector(selector) {
        if (!nodes.has(selector)) nodes.set(selector, element());
        return nodes.get(selector);
      },
      querySelectorAll: () => [],
      createElement: () => element(),
      createDocumentFragment: () => element(),
    },
    Option: function (text, value) { this.textContent = text; this.value = value; },
    requestAnimationFrame() {},
  });
  // Exercise the actual application functions. Desktop startup and canvas paint
  // require a WebView; these tests isolate state/coordinate behavior from those APIs.
  assert.match(source, /\nbootstrap\(\);\s*$/);
  vm.runInContext(source.replace(/\nbootstrap\(\);\s*$/, ""), context);
  return {
    context,
    evaluate: (code) => vm.runInContext(code, context),
    get: (name) => vm.runInContext(name, context),
  };
}

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

function chartAtScale(scale) {
  return {
    width: 420, height: 190,
    canvas: { getBoundingClientRect: () => ({ left: 100, top: 50, width: 420 * scale, height: 190 * scale }) },
    view: { baseRtStart: 0, baseRtEnd: 10, rtStart: 2, rtEnd: 6, baseYMax: 10, yMax: 10 },
  };
}

test("all static frontend ID bindings and local assets exist", () => {
  for (const [, id] of source.matchAll(/document\.querySelector\("#([\w-]+)"\)/g)) {
    assert.ok(html.includes(`id="${id}"`), `Missing UI control: ${id}`);
  }
  for (const [, asset] of html.matchAll(/(?:src|href)="([^"?#]+)(?:\?[^"#]*)?"/g)) {
    if (/^(?:[a-z]+:|#)/i.test(asset)) continue;
    assert.ok(fs.existsSync(path.join(uiDirectory, asset)), `Missing UI asset: ${asset}`);
  }
});

test("pointer RT and hit testing respect every supported GUI scale", () => {
  const app = loadApp();
  const getRt = app.get("chartRtAtClientX");
  const inside = app.get("chartPointerInsidePlot");
  for (const scale of [0.9, 1, 1.1, 1.25, 1.5, 1.75, 2]) {
    const chart = chartAtScale(scale);
    // Midpoint of the drawable plot, excluding its 54px left and 10px right margins.
    const clientX = 100 + (54 + (420 - 54 - 10) / 2) * scale;
    assert.ok(Math.abs(getRt(chart, clientX) - 4) < 1e-10, `Wrong RT at ${scale * 100}%`);
    assert.equal(inside(chart, { clientX: 100 + 409 * scale, clientY: 50 + 153 * scale }), true);
    assert.equal(inside(chart, { clientX: 100 + 53 * scale, clientY: 50 + 40 * scale }), false);
    assert.equal(inside(chart, { clientX, clientY: 50 + 155 * scale }), false);
  }
});

test("resizing charts preserves unsaved bounds, references and pending save", () => {
  const app = loadApp();
  app.evaluate(`
    drawChart = () => {};
    visualizer.data = { transition: "A", samples: [], globalIntensityMax: 10 };
    visualizer.pendingSave = { kind: "reference" };
    visualizer.charts = [{
      width: 420, height: 190, frame: { style: {} },
      integration: { rtStart: 2, rtEnd: 4 }, originalIntegration: { rtStart: 1, rtEnd: 5 },
      hasEdit: true, isReference: true, view: { rtStart: 0, rtEnd: 6, yMax: 10 },
    }];
    elements.visualizerWidth.valueAsNumber = 600;
    elements.visualizerHeight.valueAsNumber = 250;
  `);
  const chart = app.get("visualizer.charts[0]");
  const pendingSave = app.get("visualizer.pendingSave");
  app.get("redrawVisualizerCharts")();
  assert.equal(app.get("visualizer.charts[0]"), chart);
  assert.equal(chart.hasEdit, true);
  assert.equal(chart.isReference, true);
  assert.equal(chart.integration.rtStart, 2);
  assert.equal(chart.originalIntegration.rtStart, 1);
  assert.equal(chart.width, 600);
  assert.equal(chart.height, 250);
  assert.equal(chart.frame.style.width, "600px");
  assert.equal(app.get("visualizer.pendingSave"), pendingSave);
});

test("changing the displayed RT window preserves unsaved integration bounds", () => {
  const app = loadApp();
  app.evaluate(`
    drawChart = () => {};
    visualizer.data = { globalIntensityMax: 10 };
    visualizer.rangeManuallySet = true;
    visualizer.charts = [{
      width: 420, height: 190, frame: { style: {} },
      sample: { points: [{ rt: 0, intensity: 1 }, { rt: 5, intensity: 8 }, { rt: 10, intensity: 1 }] },
      well: { rtStart: 1, rtEnd: 6, startIndex: 0, endIndex: 2, height: 8 },
      integration: { rtStart: 2, rtEnd: 4 }, originalIntegration: { rtStart: 1, rtEnd: 6 },
      hasEdit: true, isReference: true, view: { rtStart: 0, rtEnd: 10, yMax: 10 },
    }];
    elements.visualizerRtStart.valueAsNumber = 3;
    elements.visualizerRtEnd.valueAsNumber = 7;
    redrawVisualizerCharts({ resetView: true });
  `);
  const chart = app.get("visualizer.charts[0]");
  assert.equal(chart.view.rtStart, 3);
  assert.equal(chart.view.rtEnd, 7);
  assert.equal(chart.integration.rtStart, 2);
  assert.equal(chart.integration.rtEnd, 4);
  assert.equal(chart.hasEdit, true);
  assert.equal(chart.isReference, true);
});

test("horizontal zoom keeps a left-edge cursor anchored to the current RT", () => {
  const app = loadApp();
  app.evaluate("drawChart = () => {};");
  for (const scale of [1, 1.5, 2]) {
    const chart = chartAtScale(scale);
    app.get("zoomChartHorizontal")(chart, 2, 100 + 54 * scale);
    assert.equal(chart.view.rtStart, 2);
    assert.equal(chart.view.rtEnd, 4);
  }
});

test("reset during chart rendering restores transition controls", () => {
  const app = loadApp();
  app.evaluate(`
    visualizer.rendering = true;
    elements.visualizerTransition.disabled = true;
    elements.visualizerRefresh.disabled = true;
    resetVisualizer();
  `);
  assert.equal(app.get("visualizer.rendering"), false);
  assert.equal(app.get("elements.visualizerTransition.disabled"), false);
  assert.equal(app.get("elements.visualizerRefresh.disabled"), false);
});

test("a slow previous transition cannot replace the newest selection", async () => {
  const pending = [deferred(), deferred()];
  let next = 0;
  const app = loadApp(() => pending[next++].promise);
  app.evaluate(`
    project = { path: "/dataset", outputs: { miscData: true } };
    renderVisualizerCharts = (data) => { visualizer.data = data; };
    elements.visualizerTransition.value = "slow";
  `);
  const first = app.get("renderSelectedTransition")();
  app.evaluate('elements.visualizerTransition.value = "new";');
  const second = app.get("renderSelectedTransition")();
  pending[1].resolve({ transition: "new" });
  await second;
  pending[0].resolve({ transition: "slow" });
  await first;
  assert.equal(app.get("visualizer.data.transition"), "new");
});

test("a project reset invalidates an in-flight transition and its errors", async () => {
  for (const fail of [false, true]) {
    const pending = deferred();
    const app = loadApp(() => pending.promise);
    app.evaluate(`
      project = { path: "/old-dataset", outputs: { miscData: true } };
      renderVisualizerCharts = (data) => { visualizer.data = data; };
      elements.visualizerTransition.value = "old";
    `);
    const request = app.get("renderSelectedTransition")();
    app.evaluate('project = { path: "/new-dataset" }; resetVisualizer();');
    if (fail) pending.reject(new Error("Old project missing"));
    else pending.resolve({ transition: "old" });
    await request;
    assert.equal(app.get("visualizer.data"), null);
    assert.equal(app.get("elements.visualizerStatus.textContent"), "Ready");
  }
});

test("a slow previous project index and its errors cannot overwrite the current project", async () => {
  for (const fail of [false, true]) {
    const pending = [deferred(), deferred()];
    let next = 0;
    const app = loadApp((command) => command === "visualizer_list_transitions"
      ? pending[next++].promise
      : Promise.resolve({ backups: [], labels: {} }));
    app.evaluate('project = { path: "/old", outputs: { miscData: true } };');
    const first = app.get("initializeVisualizer")();
    app.evaluate('project = { path: "/new", outputs: { miscData: true } };');
    const second = app.get("initializeVisualizer")();
    pending[1].resolve(["Current transition"]);
    await second;
    if (fail) pending[0].reject(new Error("Old project missing"));
    else pending[0].resolve(["Obsolete transition"]);
    await first;
    assert.equal(app.get("visualizer.projectPath"), "/new");
    assert.equal(app.get("visualizer.transitions[0]"), "Current transition");
    assert.equal(app.get("elements.visualizerStatus.textContent"), "1 transitions ready");
  }
});

test("a project reset invalidates in-flight backup lists and their errors", async () => {
  for (const fail of [false, true]) {
    const pending = deferred();
    const app = loadApp(() => pending.promise);
    app.evaluate('project = { path: "/old" };');
    const request = app.get("refreshBoundsBackups")();
    app.evaluate(`
      project = { path: "/new" };
      resetVisualizer();
      visualizer.backupLabels = { current: "Current project backup" };
    `);
    if (fail) pending.reject(new Error("Old project missing"));
    else pending.resolve({ backups: ["old"], labels: { old: "Obsolete backup" } });
    await request;
    assert.equal(app.get("visualizer.backupLabels.current"), "Current project backup");
    assert.equal(app.get("visualizer.backupLabels.old"), undefined);
  }
});
