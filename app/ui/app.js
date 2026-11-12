const tauri = window.__TAURI__;
const isDesktop = Boolean(tauri?.core?.invoke);
const invoke = isDesktop ? tauri.core.invoke : null;
const originalBackup = "acq_time_original.csv";

const elements = {
  projectTitle: document.querySelector("#project-title"),
  projectPath: document.querySelector("#project-path"),
  dataCount: document.querySelector("#data-count"),
  transitionCount: document.querySelector("#transition-count"),
  transitionFile: document.querySelector("#transition-file"),
  workerName: document.querySelector("#worker-name"),
  projectNotice: document.querySelector("#project-notice"),
  projectIssues: document.querySelector("#project-issues"),
  projectModal: document.querySelector("#project-modal"),
  modalMessage: document.querySelector("#modal-message"),
  promptModal: document.querySelector("#prompt-modal"),
  promptTitle: document.querySelector("#prompt-title"),
  promptMessage: document.querySelector("#prompt-message"),
  promptInput: document.querySelector("#prompt-input"),
  promptOk: document.querySelector("#prompt-ok"),
  promptCancel: document.querySelector("#prompt-cancel"),
  chooseButtons: [
    document.querySelector("#choose-project"),
    document.querySelector("#modal-choose-project"),
  ],
  runButton: document.querySelector("#run-all"),
  runPanel: document.querySelector("#run-panel"),
  runStatus: document.querySelector("#run-status"),
  editDataButton: document.querySelector("#edit-data-tab"),
  runTab: document.querySelector("#run-tab"),
  brandHome: document.querySelector("#brand-home"),
  visualizerTab: document.querySelector("#visualizer-tab"),
  runView: document.querySelector("#run-view"),
  visualizerView: document.querySelector("#visualizer-view"),
  visualizerDataset: document.querySelector("#visualizer-dataset"),
  visualizerSearch: document.querySelector("#visualizer-search"),
  visualizerTransition: document.querySelector("#visualizer-transition"),
  visualizerWidth: document.querySelector("#visualizer-width"),
  visualizerHeight: document.querySelector("#visualizer-height"),
  visualizerRtStart: document.querySelector("#visualizer-rt-start"),
  visualizerRtEnd: document.querySelector("#visualizer-rt-end"),
  visualizerIntensity: document.querySelector("#visualizer-intensity"),
  visualizerSave: document.querySelector("#visualizer-save"),
  visualizerOverwriteBackup: document.querySelector("#visualizer-overwrite-backup"),
  visualizerApplyReferences: document.querySelector("#visualizer-apply-references"),
  visualizerAutoShift: document.querySelector("#visualizer-auto-shift"),
  visualizerRefresh: document.querySelector("#visualizer-refresh"),
  visualizerCancel: document.querySelector("#visualizer-cancel"),
  visualizerDeleteBackup: document.querySelector("#visualizer-delete-backup"),
  visualizerRenameBackup: document.querySelector("#visualizer-rename-backup"),
  visualizerImportBackup: document.querySelector("#visualizer-import-backup"),
  visualizerBackups: document.querySelector("#visualizer-backups"),
  visualizerShortcutGlobal: document.querySelector("#visualizer-shortcuts-global"),
  visualizerAutoShiftAfterUniform: document.querySelector("#visualizer-auto-shift-after-uniform"),
  visualizerToolbar: document.querySelector(".visualizer-toolbar"),
  visualizerSelectorExpand: document.querySelector("#visualizer-selector-expand"),
  visualizerAnalyte: document.querySelector("#visualizer-analyte"),
  visualizerStatus: document.querySelector("#visualizer-status"),
  visualizerPlots: document.querySelector("#visualizer-plots"),
  visualizerEmpty: document.querySelector("#visualizer-empty"),
  themeToggle: document.querySelector("#theme-toggle"),
  guiScale: document.querySelector("#gui-scale"),
  clearActivity: document.querySelector("#clear-activity"),
  activityLog: document.querySelector("#activity-log"),
  toastRegion: document.querySelector("#toast-region"),
  externalSlingLinks: [...document.querySelectorAll(".external-sling-link")],
  dataEditorModal: document.querySelector("#data-editor-modal"),
  dataEditorFiles: document.querySelector("#data-editor-files"),
  dataEditorCurrent: document.querySelector("#data-editor-current"),
  dataEditorStatus: document.querySelector("#data-editor-status"),
  dataEditorBackups: document.querySelector("#data-editor-backups"),
  dataEditorRestore: document.querySelector("#data-editor-restore"),
  dataEditorRename: document.querySelector("#data-editor-rename"),
  dataEditorDelete: document.querySelector("#data-editor-delete"),
  dataEditorRefresh: document.querySelector("#data-editor-refresh"),
  dataEditorSave: document.querySelector("#data-editor-save"),
  dataEditorClose: document.querySelector("#data-editor-close"),
  dataEditorTableActions: document.querySelector("#data-editor-table-actions"),
  dataEditorAddRow: document.querySelector("#data-editor-add-row"),
  dataEditorAddColumn: document.querySelector("#data-editor-add-column"),
  dataEditorBody: document.querySelector("#data-editor-body"),
  outputs: {
    acqTime: document.querySelector("#output-acq"),
    resultsCsv: document.querySelector("#output-results"),
    miscData: document.querySelector("#output-misc"),
  },
};

let project = null;
let running = false;
let runFailed = false;
const visualizer = {
  projectPath: null,
  transitions: [],
  search: "",
  data: null,
  charts: [],
  hoveredChart: null,
  backupLabels: {},
  referenceIds: new Set(),
  uniformView: null,
  pendingSave: null,
  chartColors: null,
  projectToken: 0,
  loadToken: 0,
  renderToken: 0,
  rendering: false,
  rangeManuallySet: false,
};

const chartMargins = { left: 54, right: 10, top: 18, bottom: 36 };
const autoShift = {
  points: 64,
  maxShift: 0.22,
  minScore: 0.78,
  minImprovement: 0.025,
  minSignal: 1,
  apexFractionTolerance: 0.28,
};

const dataEditor = {
  files: [],
  current: null,
  content: null,
  viewingBackup: "",
  dirty: false,
};

const mockProject = {
  name: "EchoMSkit-Dataset",
  path: "C:\\Examples\\EchoMSkit-Dataset",
  dataCount: 2,
  transitionCount: 42,
  transitionFile: "transitions.csv",
  workerName: "EchoMSkit.exe",
  issues: [],
  canRun: true,
  setup: [],
  outputs: {
    acqTime: false,
    resultsCsv: false,
    miscData: false,
  },
};

// browser preview data shaped like an echo run, 30 s ejections about 0.84 min apart
const mockVisualizerData = (() => {
  const period = 0.0496;
  const spacing = 0.843;
  const samples = Array.from({ length: 2 }, (_, sampleIndex) => {
    const centers = Array.from({ length: 5 }, (_, well) => 0.84 + well * spacing);
    const points = Array.from({ length: 96 }, (_, index) => {
      const rt = 0.3 + index * period;
      const lit = centers.some((center) => Math.abs(rt - center) < 0.25);
      const ripple = Math.sin(index * 2.3 + sampleIndex) * 6000;
      return { rt, intensity: lit ? 78000 + ripple : 0 };
    });
    const wells = centers.map((center, well) => {
      const startIndex = points.findLastIndex((point) => point.rt <= center - 0.25);
      const endIndex = points.findIndex((point) => point.rt >= center + 0.25);
      return {
        label: `${well + 1} B${sampleIndex + 2}`,
        sampleId: `SN03${sampleIndex}${well}`,
        startIndex,
        endIndex,
        rtStart: points[startIndex].rt,
        rtEnd: points[endIndex].rt,
        apexRt: center,
        height: 84000,
        area: 2300000,
      };
    });
    return {
      sampleId: `run${sampleIndex + 1}.wiff`,
      displayName: `run${sampleIndex + 1}`,
      points,
      wells,
    };
  });
  return {
    transition: "Mock transition",
    sampleCount: samples.length,
    wellCount: samples.length * 5,
    globalRtMin: 0.3,
    globalRtMax: 0.3 + 95 * period,
    globalIntensityMax: 84000,
    samples,
  };
})();

// adds one line to the run log
function addActivity(message, kind = "") {
  const empty = elements.activityLog.querySelector(".empty-log");
  if (empty) empty.remove();
  const row = document.createElement("p");
  row.className = kind;
  const timestamp = new Date().toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
  row.textContent = `${timestamp}  ${message}`;
  elements.activityLog.append(row);
  elements.activityLog.scrollTop = elements.activityLog.scrollHeight;
}

// shows a short status toast
function showToast(message, kind = "") {
  const toast = document.createElement("div");
  toast.className = `toast ${kind}`.trim();
  toast.textContent = message;
  elements.toastRegion.append(toast);
  window.setTimeout(() => toast.remove(), 4200);
}

// uses the in-app prompt for backup names
function appPrompt(message, defaultValue = "", title = "Rename backup") {
  return new Promise((resolve) => {
    elements.promptTitle.textContent = title;
    elements.promptMessage.textContent = message;
    elements.promptInput.value = defaultValue;
    elements.promptModal.classList.remove("hidden");
    elements.promptInput.focus();
    elements.promptInput.select();

    const cleanup = (value) => {
      elements.promptModal.classList.add("hidden");
      elements.promptOk.removeEventListener("click", ok);
      elements.promptCancel.removeEventListener("click", cancel);
      elements.promptInput.removeEventListener("keydown", keydown);
      resolve(value);
    };
    const ok = () => cleanup(elements.promptInput.value);
    const cancel = () => cleanup(null);
    const keydown = (event) => {
      if (event.key === "Enter") ok();
      if (event.key === "Escape") cancel();
    };
    elements.promptOk.addEventListener("click", ok);
    elements.promptCancel.addEventListener("click", cancel);
    elements.promptInput.addEventListener("keydown", keydown);
  });
}

// formats counts for compact stat cards
function formatCount(value) {
  return Number.isFinite(value) ? value.toLocaleString() : "-";
}

// toggles one output readiness pill
function setOutput(element, complete) {
  element.classList.toggle("complete", complete);
  element.classList.toggle("pending", !complete);
  element.querySelector("strong").textContent = complete ? "ready" : "waiting";
}

// mirrors run and output state on the processing step
function updateRunStatus() {
  const outputs = Object.keys(elements.outputs);
  const complete = Boolean(project) && outputs.every((key) => project.outputs?.[key]);
  const status = running
    ? "running"
    : runFailed
      ? "failed"
      : complete
        ? "complete"
        : "waiting";
  elements.runStatus.className = `step-status ${status}`;
  elements.runStatus.textContent = status;
}

// updates the run page from backend project state
function renderProject(summary) {
  const previousPath = project?.path;
  project = summary;
  elements.projectTitle.textContent = summary.name;
  elements.projectPath.textContent = summary.path;
  elements.dataCount.textContent = formatCount(summary.dataCount);
  elements.transitionCount.textContent = formatCount(summary.transitionCount);
  elements.transitionFile.textContent = summary.transitionFile ?? "not found";
  elements.workerName.textContent = summary.workerName ?? "not found";
  if (previousPath && previousPath !== summary.path) {
    runFailed = false;
    resetVisualizer();
  }

  elements.projectIssues.replaceChildren();
  if (summary.issues.length) {
    for (const issue of summary.issues) {
      const item = document.createElement("li");
      item.textContent = issue;
      elements.projectIssues.append(item);
    }
    elements.projectNotice.classList.remove("hidden");
  } else {
    elements.projectNotice.classList.add("hidden");
  }

  for (const [key, element] of Object.entries(elements.outputs)) {
    setOutput(element, Boolean(summary.outputs?.[key]));
  }

  elements.runButton.disabled = running || !summary.canRun;
  updateRunStatus();
  updateSaveButton();
  updateBackupButtons();
  elements.projectModal.classList.add("hidden");
}

// locks run controls while EchoMSkit is active
function setRunning(value) {
  running = value;
  elements.runButton.classList.toggle("running", value);
  elements.runButton.disabled = value || !project?.canRun;
  updateRunStatus();
  updateSaveButton();
  updateBackupButtons();
}

// lets the user pick a dataset folder
async function chooseProject() {
  if (!isDesktop) {
    renderProject(mockProject);
    addActivity("Browser preview project selected.");
    return;
  }

  try {
    const path = await tauri.dialog.open({
      directory: true,
      multiple: false,
      title: "Choose an EchoMSkit project folder",
    });
    if (!path) return;
    const summary = await invoke("select_project", { path });
    renderProject(summary);
    addActivity(`Selected ${summary.name}.`);
    logSetup(summary);
    if (summary.setup.length) {
      showToast("The folder was set up for EchoMSkit, see the activity log.");
    }
    if (!summary.canRun) {
      showToast("The project was selected, but some required files are missing.", "error");
    }
  } catch (error) {
    showToast(String(error), "error");
  }
}

// reloads project outputs after external changes
async function refreshProject() {
  if (!project) return;
  if (!isDesktop) {
    renderProject({ ...project });
    return;
  }
  const summary = await invoke("refresh_project", { path: project.path });
  renderProject(summary);
  logSetup(summary);
}

// files moved or written while scanning a raw export
function logSetup(summary) {
  for (const note of summary.setup) {
    addActivity(note, note.includes("could not") ? "error" : "success");
  }
}

// runs the EchoMSkit worker and refreshes the gui
async function runAll() {
  if (!project || running) return;
  if (!project.canRun) {
    showToast("Resolve the project issues before running EchoMSkit.", "error");
    return;
  }
  runFailed = false;
  setRunning(true);
  addActivity("EchoMSkit run started.");
  try {
    if (!isDesktop) {
      await new Promise((resolve) => window.setTimeout(resolve, 900));
      renderProject({
        ...project,
        outputs: {
          acqTime: true,
          resultsCsv: true,
          miscData: true,
        },
      });
      resetVisualizer();
    } else {
      const summary = await invoke("run_all", { path: project.path });
      renderProject(summary);
      resetVisualizer();
    }
    addActivity("EchoMSkit run completed successfully.", "success");
    showToast("EchoMSkit run complete.");
  } catch (error) {
    runFailed = true;
    addActivity(String(error), "error");
    showToast(String(error), "error");
    try {
      await refreshProject();
    } catch (refreshError) {
      addActivity(`Refresh failed: ${String(refreshError)}`, "error");
    }
  } finally {
    setRunning(false);
  }
}

// updates the data editor status line
function dataEditorStatus(message) {
  elements.dataEditorStatus.textContent = message;
}

// shows editable files found in the dataset
function renderDataEditorFiles() {
  elements.dataEditorFiles.replaceChildren();
  for (const file of dataEditor.files) {
    const button = document.createElement("button");
    button.className = "data-editor-file";
    button.type = "button";
    button.classList.toggle("active", dataEditor.current?.kind === file.kind);

    const title = document.createElement("strong");
    title.textContent = file.title;
    const name = document.createElement("span");
    name.textContent = file.name;
    button.append(title, name);
    button.addEventListener("click", () => openDataEditorFile(file.kind));
    elements.dataEditorFiles.append(button);
  }
}

// recognizes the protected original backup
function isDataEditorOriginalBackup(name) {
  return /^original\.[^.]+$/i.test(name ?? "");
}

// renders the backup menu for the current file
function renderDataEditorBackups(backups = []) {
  elements.dataEditorBackups.replaceChildren(new Option("Current file", ""));
  for (const backup of backups) {
    elements.dataEditorBackups.append(new Option(backup, backup));
  }
  elements.dataEditorBackups.value = dataEditor.viewingBackup;
  const hasCurrent = Boolean(dataEditor.current);
  const hasBackup = Boolean(dataEditor.viewingBackup);
  const protectedBackup = isDataEditorOriginalBackup(dataEditor.viewingBackup);
  elements.dataEditorBackups.disabled = !hasCurrent || backups.length === 0;
  elements.dataEditorRestore.disabled = !hasBackup;
  elements.dataEditorRename.disabled = !hasBackup || protectedBackup;
  elements.dataEditorDelete.disabled = !hasBackup || protectedBackup;
}

// marks table or text edits as pending
function markDataEditorDirty() {
  if (!dataEditor.current || dataEditor.viewingBackup) return;
  dataEditor.dirty = true;
  elements.dataEditorSave.disabled = false;
  dataEditorStatus("Unsaved changes");
}

// builds a table cell input
function dataEditorCell(value, onInput) {
  const input = document.createElement("input");
  input.type = "text";
  input.value = value ?? "";
  input.disabled = Boolean(dataEditor.viewingBackup);
  input.addEventListener("input", onInput);
  return input;
}

// reads the current editor controls
function collectDataEditorInput() {
  const file = dataEditor.current;
  if (!file) return null;
  if (file.format === "text") {
    return {
      kind: file.kind,
      text: elements.dataEditorBody.querySelector("textarea")?.value ?? "",
      headers: [],
      rows: [],
    };
  }

  const headers = [...elements.dataEditorBody.querySelectorAll("thead input")].map(
    (input) => input.value,
  );
  const rows = [...elements.dataEditorBody.querySelectorAll("tbody tr")].map((row) =>
    [...row.querySelectorAll("td input")].map((input) => input.value),
  );
  return { kind: file.kind, text: null, headers, rows };
}

// renders the current text or csv editor
function renderDataEditorContent(content) {
  dataEditor.content = content;
  dataEditor.current = content.file;
  dataEditor.dirty = false;
  renderDataEditorFiles();
  renderDataEditorBackups(content.backups);
  elements.dataEditorCurrent.textContent = `${content.file.title} - ${content.file.name}`;
  elements.dataEditorSave.disabled = Boolean(dataEditor.viewingBackup);
  dataEditorStatus(
    dataEditor.viewingBackup ? `Viewing ${dataEditor.viewingBackup}` : (content.note ?? "Ready"),
  );
  elements.dataEditorBody.replaceChildren();
  elements.dataEditorTableActions.classList.toggle(
    "hidden",
    content.file.format !== "csv" || Boolean(dataEditor.viewingBackup),
  );

  if (content.file.format === "text") {
    const textarea = document.createElement("textarea");
    textarea.className = "data-editor-textarea";
    textarea.value = content.text ?? "";
    textarea.disabled = Boolean(dataEditor.viewingBackup);
    textarea.addEventListener("input", markDataEditorDirty);
    elements.dataEditorBody.append(textarea);
    return;
  }

  const headers = content.headers.length ? content.headers : ["Column 1"];
  const rows = content.rows ?? [];
  const wrapper = document.createElement("div");
  wrapper.className = "data-editor-table-wrap";
  const table = document.createElement("table");
  table.className = "data-editor-table";
  const thead = document.createElement("thead");
  const headerRow = document.createElement("tr");
  const corner = document.createElement("th");
  corner.className = "row-number";
  corner.textContent = "#";
  headerRow.append(corner);
  for (const header of headers) {
    const th = document.createElement("th");
    th.append(dataEditorCell(header, markDataEditorDirty));
    headerRow.append(th);
  }
  thead.append(headerRow);

  const tbody = document.createElement("tbody");
  rows.forEach((row, rowIndex) => {
    const tr = document.createElement("tr");
    const rowNumber = document.createElement("th");
    rowNumber.className = "row-number";
    const deleteButton = document.createElement("button");
    deleteButton.className = "delete-row";
    deleteButton.type = "button";
    deleteButton.textContent = String(rowIndex + 1);
    deleteButton.title = "delete row";
    deleteButton.disabled = Boolean(dataEditor.viewingBackup);
    deleteButton.addEventListener("click", () => {
      tr.remove();
      [...tbody.querySelectorAll(".delete-row")].forEach((button, index) => {
        button.textContent = String(index + 1);
      });
      markDataEditorDirty();
    });
    rowNumber.append(deleteButton);
    tr.append(rowNumber);
    for (let column = 0; column < headers.length; column += 1) {
      const td = document.createElement("td");
      td.append(dataEditorCell(row[column] ?? "", markDataEditorDirty));
      tr.append(td);
    }
    tbody.append(tr);
  });

  table.append(thead, tbody);
  wrapper.append(table);
  elements.dataEditorBody.append(wrapper);
}

// opens the editor modal and loads file metadata
async function openDataEditor() {
  if (!project) {
    showToast("Select a dataset folder first.", "error");
    return;
  }
  elements.dataEditorModal.classList.remove("hidden");
  elements.editDataButton.classList.add("active");
  dataEditorStatus("Loading files...");
  if (!isDesktop) {
    showToast("The data editor is available in the desktop app.");
    return;
  }
  try {
    dataEditor.files = await invoke("data_editor_files", { projectPath: project.path });
    renderDataEditorFiles();
    const first = dataEditor.files[0];
    if (first) {
      await openDataEditorFile(dataEditor.current?.kind ?? first.kind);
    } else {
      dataEditorStatus("No editable files found");
    }
  } catch (error) {
    dataEditorStatus("Failed to load files");
    showToast(String(error), "error");
  }
}

// closes the editor modal
function closeDataEditor() {
  elements.dataEditorModal.classList.add("hidden");
  elements.editDataButton.classList.remove("active");
}

// loads one editable live file
async function openDataEditorFile(kind) {
  if (!project || !isDesktop) return;
  try {
    dataEditor.viewingBackup = "";
    dataEditorStatus("Loading file...");
    const content = await invoke("data_editor_read", {
      projectPath: project.path,
      kind,
    });
    renderDataEditorContent(content);
  } catch (error) {
    dataEditorStatus("Failed to load file");
    showToast(String(error), "error");
  }
}

// previews a saved backup
async function openDataEditorBackup(name) {
  if (!project || !dataEditor.current || !isDesktop) return;
  if (!name) {
    await openDataEditorFile(dataEditor.current.kind);
    return;
  }
  try {
    dataEditor.viewingBackup = name;
    dataEditorStatus("Loading backup...");
    const content = await invoke("data_editor_read_backup", {
      projectPath: project.path,
      kind: dataEditor.current.kind,
      backup: name,
    });
    renderDataEditorContent(content);
  } catch (error) {
    dataEditorStatus("Failed to load backup");
    showToast(String(error), "error");
  }
}

// saves the current live file
async function saveDataEditorFile() {
  const input = collectDataEditorInput();
  if (!project || !input || dataEditor.viewingBackup || !isDesktop) return;
  try {
    dataEditorStatus("Saving...");
    const content = await invoke("data_editor_save", {
      projectPath: project.path,
      input,
    });
    dataEditor.viewingBackup = "";
    renderDataEditorContent(content);
    await refreshProject();
    resetVisualizer();
    addActivity(`Saved ${content.file.title}.`, "success");
    showToast(`${content.file.title} saved.`);
  } catch (error) {
    dataEditorStatus("Save failed");
    showToast(String(error), "error");
  }
}

// reloads the selected file from disk
async function refreshDataEditorFile() {
  if (!dataEditor.current) return;
  if (dataEditor.viewingBackup) {
    await openDataEditorBackup(dataEditor.viewingBackup);
  } else {
    await openDataEditorFile(dataEditor.current.kind);
  }
}

// restores the selected backup into the live file
async function restoreDataEditorBackup() {
  if (!project || !dataEditor.current || !dataEditor.viewingBackup || !isDesktop) return;
  try {
    const backupName = dataEditor.viewingBackup;
    dataEditorStatus("Restoring backup...");
    const content = await invoke("data_editor_restore_backup", {
      projectPath: project.path,
      kind: dataEditor.current.kind,
      backup: backupName,
    });
    dataEditor.viewingBackup = "";
    renderDataEditorContent(content);
    await refreshProject();
    resetVisualizer();
    addActivity(`Restored ${backupName}.`, "success");
    showToast("Backup restored.");
  } catch (error) {
    dataEditorStatus("Restore failed");
    showToast(String(error), "error");
  }
}

// renames a selected backup
async function renameDataEditorBackup() {
  if (!project || !dataEditor.current || !dataEditor.viewingBackup || !isDesktop) return;
  if (isDataEditorOriginalBackup(dataEditor.viewingBackup)) return;
  const nextName = await appPrompt(
    "Rename this data file backup.",
    dataEditor.viewingBackup,
    "Rename data backup",
  );
  if (!nextName || nextName === dataEditor.viewingBackup) return;
  try {
    const newName = await invoke("data_editor_rename_backup", {
      projectPath: project.path,
      kind: dataEditor.current.kind,
      backup: dataEditor.viewingBackup,
      name: nextName,
    });
    dataEditor.viewingBackup = newName;
    await openDataEditorBackup(newName);
    showToast("Backup renamed.");
  } catch (error) {
    showToast(String(error), "error");
  }
}

// deletes a selected backup
async function deleteDataEditorBackup() {
  if (!project || !dataEditor.current || !dataEditor.viewingBackup || !isDesktop) return;
  if (isDataEditorOriginalBackup(dataEditor.viewingBackup)) return;
  try {
    const kind = dataEditor.current.kind;
    const deleted = dataEditor.viewingBackup;
    await invoke("data_editor_delete_backup", {
      projectPath: project.path,
      kind,
      backup: deleted,
    });
    dataEditor.viewingBackup = "";
    await openDataEditorFile(kind);
    showToast("Backup deleted.");
  } catch (error) {
    showToast(String(error), "error");
  }
}

// adds an empty csv row
function addDataEditorRow() {
  if (!dataEditor.current || dataEditor.current.format !== "csv" || dataEditor.viewingBackup) {
    return;
  }
  const input = collectDataEditorInput();
  dataEditor.content.headers = input.headers;
  dataEditor.content.rows = [...input.rows, input.headers.map(() => "")];
  renderDataEditorContent(dataEditor.content);
  markDataEditorDirty();
}

// adds an empty csv column
function addDataEditorColumn() {
  if (!dataEditor.current || dataEditor.current.format !== "csv" || dataEditor.viewingBackup) {
    return;
  }
  const input = collectDataEditorInput();
  const headers = [...input.headers, `Column ${input.headers.length + 1}`];
  const rows = input.rows.map((row) => [...row, ""]);
  dataEditor.content.headers = headers;
  dataEditor.content.rows = rows;
  renderDataEditorContent(dataEditor.content);
  markDataEditorDirty();
}

// switches to the run tab
function showRunView() {
  elements.visualizerView.classList.add("hidden");
  elements.runView.classList.remove("hidden");
  elements.runTab.classList.add("active");
  elements.runTab.setAttribute("aria-current", "page");
  elements.visualizerTab.classList.remove("active");
  elements.visualizerTab.removeAttribute("aria-current");
}

// switches to the visualizer tab
function showVisualizerView() {
  elements.runView.classList.add("hidden");
  elements.visualizerView.classList.remove("hidden");
  elements.visualizerTab.classList.add("active");
  elements.visualizerTab.setAttribute("aria-current", "page");
  elements.runTab.classList.remove("active");
  elements.runTab.removeAttribute("aria-current");
  initializeVisualizer();
}

// writes the visualizer footer message
function visualizerMessage(message) {
  elements.visualizerStatus.textContent = message;
}

// lets status text paint during long visualizer work
function nextFrame() {
  return new Promise((resolve) => window.requestAnimationFrame(resolve));
}

// shows one numbered visualizer work step
async function visualizerStep(step, total, message) {
  visualizerMessage(`Step ${step}/${total}: ${message}`);
  await nextFrame();
}

// yields during large chart loops
async function visualizerLoopStep(index, total, step, stepTotal, message) {
  if (index % 80 !== 0 && index + 1 !== total) return;
  visualizerMessage(
    `Step ${step}/${stepTotal}: ${message} ${Math.min(index + 1, total).toLocaleString()} / ${total.toLocaleString()}`,
  );
  await nextFrame();
}

// clears visualizer state for a new project
function resetVisualizer() {
  visualizer.projectPath = null;
  visualizer.transitions = [];
  visualizer.search = "";
  visualizer.data = null;
  visualizer.charts = [];
  visualizer.hoveredChart = null;
  visualizer.backupLabels = {};
  visualizer.referenceIds.clear();
  visualizer.uniformView = null;
  visualizer.pendingSave = null;
  visualizer.chartColors = null;
  visualizer.projectToken += 1;
  visualizer.loadToken += 1;
  visualizer.renderToken += 1;
  visualizer.rendering = false;
  visualizer.rangeManuallySet = false;
  elements.visualizerSearch.value = "";
  elements.visualizerTransition.replaceChildren(new Option("Select a transition", ""));
  elements.visualizerTransition.disabled = false;
  elements.visualizerRefresh.disabled = false;
  elements.visualizerBackups.replaceChildren(new Option("No saved versions yet", ""));
  elements.visualizerBackups.disabled = true;
  elements.visualizerSave.disabled = true;
  elements.visualizerOverwriteBackup.disabled = true;
  elements.visualizerApplyReferences.disabled = true;
  elements.visualizerAutoShift.disabled = true;
  elements.visualizerDeleteBackup.disabled = true;
  elements.visualizerRenameBackup.disabled = true;
  elements.visualizerPlots.replaceChildren();
  elements.visualizerView.classList.remove("has-plots");
  elements.visualizerEmpty.classList.remove("hidden");
  elements.visualizerAnalyte.textContent = "Choose a transition to render EchoMSkit chromatograms.";
  elements.visualizerDataset.textContent = project?.path ?? "Select a processed dataset in Workflow first.";
  elements.visualizerCancel.classList.add("hidden");
  visualizerMessage("Ready");
  updateSaveButton();
  updateBackupButtons();
}

// builds filesystem-safe backup timestamps
function fileTimestamp(date = new Date()) {
  const pad = (value) => String(value).padStart(2, "0");
  return (
    `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}_` +
    `${pad(date.getHours())}-${pad(date.getMinutes())}-${pad(date.getSeconds())}`
  );
}

// names regular integration backups
function backupFileName(date = new Date()) {
  return `acq_time_${fileTimestamp(date)}.csv`;
}

// finds the next reference or autoshift number
function nextNumberedBackupNumber(prefix) {
  const numbers = [...elements.visualizerBackups.options]
    .map((option) => option.value.match(new RegExp(`^${prefix}(\\d+)_`)))
    .filter(Boolean)
    .map((match) => Number(match[1]))
    .filter(Number.isFinite);
  return numbers.length ? Math.max(...numbers) + 1 : 1;
}

// names numbered reference and autoshift backups
function numberedBackupFileName(prefix, number, date = new Date()) {
  return `${prefix}${number}_${fileTimestamp(date)}.csv`;
}

// prepares the filename and visible backup label
function numberedBackupPlan(prefix, date = new Date()) {
  const number = nextNumberedBackupNumber(prefix);
  return {
    name: numberedBackupFileName(prefix, number, date),
    label: `${prefix}${number}: ${displayTimestamp(date)}`,
    number,
  };
}

// formats timestamps for the backup menu
function displayTimestamp(date = new Date()) {
  const pad = (value) => String(value).padStart(2, "0");
  return (
    `${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}  ` +
    `${pad(date.getMonth() + 1)}/${pad(date.getDate())}/${date.getFullYear()}`
  );
}

// derives labels for older unlabeled backups
function backupLabelFallback(name) {
  if (name === originalBackup) {
    return "Original acq_time.csv";
  }
  const numberedMatch = name.match(/^(reference|autoshift)(\d+)_(\d{4})-(\d{2})-(\d{2})_(\d{2})-(\d{2})-(\d{2})/);
  if (numberedMatch) {
    const [, prefix, number, year, month, day, hour, minute, second] = numberedMatch;
    return `${prefix}${number}: ${hour}:${minute}:${second}  ${month}/${day}/${year}`;
  }
  const match = name.match(/(\d{4})-(\d{2})-(\d{2})_(\d{2})-(\d{2})-(\d{2})/);
  if (!match) return name;
  const [, year, month, day, hour, minute, second] = match;
  return `${hour}:${minute}:${second}  ${month}/${day}/${year}`;
}

// prefers renamed backup labels when present
function backupLabel(name) {
  return visualizer.backupLabels[name] ?? backupLabelFallback(name);
}

// enables saving only when bounds changed
function updateSaveButton() {
  const dirty = visualizer.charts.some((chart) => chart.hasEdit);
  elements.visualizerSave.disabled = !dirty || visualizer.rendering || running;
  elements.visualizerSave.title =
    dirty && visualizer.pendingSave?.kind === "reference"
      ? "save reference integration bounds"
      : "save dragged integration bounds";
  if (elements.visualizerOverwriteBackup) {
    updateBackupButtons();
  }
  elements.visualizerToolbar.classList.toggle("has-unsaved", dirty);
}

// enables reference actions when plots are selected
function updateReferenceButton() {
  const count = visualizer.charts.filter((chart) => chart.isReference).length;
  elements.visualizerApplyReferences.disabled =
    count === 0 || visualizer.rendering || running || !project?.path;
  elements.visualizerAutoShift.disabled =
    count === 0 || visualizer.rendering || running || !project?.path;
  elements.visualizerApplyReferences.title = count
    ? `apply ${count.toLocaleString()} selected reference integration(s) to this transition`
    : "select reference plots to apply a uniform integration";
  elements.visualizerAutoShift.title = count
    ? `auto-shift plots with ${count.toLocaleString()} selected reference profile(s)`
    : "select reference plots to auto-shift this transition";
}

// protects original and disables invalid backup actions
function updateBackupButtons() {
  const name = elements.visualizerBackups.value;
  const hasBackup = Boolean(name);
  const isOriginal = name === originalBackup;
  const dirty = visualizer.charts.some((chart) => chart.hasEdit);
  elements.visualizerBackups.disabled = !hasBackup || visualizer.rendering || running;
  elements.visualizerDeleteBackup.disabled = !hasBackup || isOriginal || visualizer.rendering || running;
  elements.visualizerRenameBackup.disabled = !hasBackup || isOriginal || visualizer.rendering || running;
  elements.visualizerOverwriteBackup.disabled =
    !dirty || !hasBackup || isOriginal || visualizer.rendering || running;
  elements.visualizerDeleteBackup.title = isOriginal
    ? "Original acq_time.csv is protected"
    : "delete selected saved version";
  elements.visualizerRenameBackup.title = isOriginal
    ? "Original acq_time.csv cannot be renamed"
    : "rename selected saved version";
  elements.visualizerOverwriteBackup.title = isOriginal
    ? "Original acq_time.csv is protected"
    : hasBackup
      ? "overwrite selected saved version with current edits"
      : "select a saved version to overwrite";
  updateReferenceButton();
}

// reloads backup choices from disk
async function refreshBoundsBackups(selected = null) {
  if (!project?.path || !isDesktop) {
    visualizer.backupLabels = {};
    elements.visualizerBackups.replaceChildren(new Option("No saved versions yet", ""));
    updateBackupButtons();
    return "";
  }
  const projectPath = project.path;
  const projectToken = visualizer.projectToken;
  try {
    const list = await invoke("visualizer_list_bounds_backups", {
      projectPath,
    });
    if (projectToken !== visualizer.projectToken || project?.path !== projectPath) return "";
    visualizer.backupLabels = list.labels ?? {};
    const fragment = document.createDocumentFragment();
    if (!list.backups.length) {
      fragment.append(new Option("No saved versions yet", ""));
    } else {
      for (const name of list.backups) {
        const label = backupLabel(name);
        const text = visualizer.backupLabels[name]
          ? `${label} (${backupLabelFallback(name)})`
          : label;
        fragment.append(new Option(text, name));
      }
    }
    elements.visualizerBackups.replaceChildren(fragment);
    const requested = selected && list.backups.includes(selected) ? selected : null;
    const remembered = list.last && list.backups.includes(list.last) ? list.last : null;
    const original = list.backups.includes(originalBackup) ? originalBackup : "";
    elements.visualizerBackups.value = requested ?? remembered ?? original;
  } catch {
    if (projectToken !== visualizer.projectToken || project?.path !== projectPath) return "";
    visualizer.backupLabels = {};
    elements.visualizerBackups.replaceChildren(new Option("No saved versions yet", ""));
    updateBackupButtons();
    return "";
  }
  updateBackupButtons();
  return elements.visualizerBackups.value;
}

// returns to the user's last scroll position
function restoreVisualizerScroll(scrollY) {
  if (scrollY == null) return;
  window.requestAnimationFrame(() => {
    window.requestAnimationFrame(() => {
      window.scrollTo({
        top: scrollY,
        left: window.scrollX,
        behavior: "auto",
      });
    });
  });
}

// saves edits as a new backup or overwrites the selected backup
async function saveBounds(options = {}) {
  if (!project?.path || running || visualizer.rendering) return;
  const overwriteName = options.overwrite ? elements.visualizerBackups.value : "";
  if (options.overwrite && (!overwriteName || overwriteName === originalBackup)) {
    showToast("Select a non-original saved version to overwrite.", "error");
    return;
  }
  const edits = visualizer.charts
    .filter((chart) => chart.hasEdit)
    .map((chart) => ({
      sampleId: chart.sample.sampleId,
      wellLabel: chart.well.label,
      rtStart: chart.integration.rtStart,
      rtEnd: chart.integration.rtEnd,
    }));
  if (!edits.length) return;

  const scrollY = window.scrollY;
  const plan =
    options.overwrite
      ? { name: overwriteName, label: null, overwrite: true }
      : visualizer.pendingSave?.kind === "reference"
      ? numberedBackupPlan("reference")
      : { name: backupFileName(), label: null };
  updateSaveButton();
  updateBackupButtons();
  visualizerMessage(
    options.overwrite
      ? `Overwriting ${backupLabel(overwriteName)} with ${edits.length.toLocaleString()} edited integration bound(s)...`
      : `Saving ${edits.length.toLocaleString()} edited integration bound(s)...`,
  );
  try {
    const result = isDesktop
      ? await invoke("visualizer_save_bounds", {
          projectPath: project.path,
          edits,
          backupName: plan.name,
        })
      : {
          written: edits.length,
          backup: plan.name,
          project: {
            ...project,
            outputs: { ...project.outputs, acqTime: true, miscData: true },
          },
        };
    if (plan.label && result.backup) {
      if (isDesktop) {
        await invoke("visualizer_rename_bounds_backup", {
          projectPath: project.path,
          name: result.backup,
          label: plan.label,
        });
      } else {
        visualizer.backupLabels[result.backup] = plan.label;
      }
    }
    renderProject(result.project);
    addActivity(
      options.overwrite
        ? `Overwrote ${backupLabel(overwriteName)} with ${result.written.toLocaleString()} EchoMSkit integration edit(s).`
        : `Saved ${result.written.toLocaleString()} EchoMSkit integration edit(s).`,
      "success",
    );
    showToast(
      options.overwrite
        ? "Selected version overwritten and EchoMSkit outputs refreshed."
        : "Integration bounds saved and EchoMSkit outputs refreshed.",
    );
    await refreshBoundsBackups(result.backup);
    visualizer.pendingSave = null;
    for (const chart of visualizer.charts) {
      chart.originalIntegration = { ...chart.integration };
      chart.hasEdit = false;
    }
    updateSaveButton();
    await renderSelectedTransition({ scrollY });
  } catch (error) {
    visualizerMessage(String(error));
    addActivity(String(error), "error");
    showToast(String(error), "error");
  } finally {
    updateSaveButton();
    updateBackupButtons();
  }
}

// keeps reference saves on one shared backend path
async function saveIntegrationEdits(edits, backupName, backupLabel, scrollY, statusMessage, successMessage) {
  if (!project?.path || running || visualizer.rendering || !edits.length) return null;
  const keepUniformView = Boolean(visualizer.uniformView);
  visualizerMessage(statusMessage);
  try {
    const result = isDesktop
      ? await invoke("visualizer_save_bounds", {
          projectPath: project.path,
          edits,
          backupName,
        })
      : {
          written: edits.length,
          backup: backupName,
          project: {
            ...project,
            outputs: { ...project.outputs, acqTime: true, miscData: true },
          },
        };
    if (backupLabel && result.backup) {
      if (isDesktop) {
        await invoke("visualizer_rename_bounds_backup", {
          projectPath: project.path,
          name: result.backup,
          label: backupLabel,
        });
      } else {
        visualizer.backupLabels[result.backup] = backupLabel;
      }
    }
    renderProject(result.project);
    addActivity(successMessage(result), "success");
    showToast("Integration bounds saved and EchoMSkit outputs refreshed.");
    await refreshBoundsBackups(result.backup);
    await renderSelectedTransition({ scrollY, keepUniformView });
    return result;
  } catch (error) {
    visualizerMessage(String(error));
    addActivity(String(error), "error");
    showToast(String(error), "error");
    return null;
  } finally {
    updateSaveButton();
    updateBackupButtons();
  }
}

// applies the average reference window to every plot
async function applyReferenceIntegration() {
  if (!project?.path || running || visualizer.rendering) return;
  const references = visualizer.charts.filter((chart) => chart.isReference);
  if (!references.length) {
    showToast("Select at least one reference plot first.", "error");
    return;
  }
  const windows = references.map((chart) => {
    const domain = visualizerDomain(chart);
    return {
      startOffset: chart.integration.rtStart - domain.rtStart,
      endOffset: chart.integration.rtEnd - domain.rtStart,
      viewSpan: domain.rtEnd - domain.rtStart,
    };
  });
  const averageStartOffset =
    windows.reduce((sum, window) => sum + window.startOffset, 0) / windows.length;
  const averageEndOffset =
    windows.reduce((sum, window) => sum + window.endOffset, 0) / windows.length;
  const averageViewSpan =
    windows.reduce((sum, window) => sum + window.viewSpan, 0) / windows.length;
  if (
    !Number.isFinite(averageStartOffset) ||
    !Number.isFinite(averageEndOffset) ||
    !Number.isFinite(averageViewSpan) ||
    averageEndOffset <= averageStartOffset
  ) {
    showToast("The selected references do not have a valid integration range.", "error");
    return;
  }

  const scrollY = window.scrollY;
  const autoShiftEnabled = elements.visualizerAutoShiftAfterUniform.checked;
  const stepTotal = autoShiftEnabled ? 4 : 2;
  await visualizerStep(1, stepTotal, "averaging selected reference bounds...");
  const referenceProfile = averageReferenceProfile(references);
  await visualizerStep(2, stepTotal, "applying uniform bounds to rendered plots...");
  visualizer.uniformView = null;
  for (let index = 0; index < visualizer.charts.length; index += 1) {
    const chart = visualizer.charts[index];
    const domain = visualizerDomain(chart);
    setChartIntegrationByRt(
      chart,
      domain.rtStart + averageStartOffset,
      domain.rtStart + averageEndOffset,
    );
    chart.hasEdit = true;
    drawChart(chart);
    syncChartReferenceState(chart);
    await visualizerLoopStep(
      index,
      visualizer.charts.length,
      2,
      stepTotal,
      "applying uniform bounds...",
    );
  }
  updateSaveButton();
  visualizer.pendingSave = { kind: "reference" };
  if (!autoShiftEnabled) {
    visualizerMessage(
      `Applied reference offsets +${averageStartOffset.toFixed(3)} to +${averageEndOffset.toFixed(3)}. Save is ready.`,
    );
    showToast("Uniform reference integration staged. Press Save to write it.");
    updateSaveButton();
    updateBackupButtons();
    return;
  }

  let shiftedCount = 0;
  let averageScore = null;
  if (referenceProfile) {
    await visualizerStep(3, stepTotal, "shifting necessary plots based on cosine similarity score...");
    const shifts = [];
    const candidates = visualizer.charts.filter((item) => !item.isReference);
    for (let index = 0; index < candidates.length; index += 1) {
      const chart = candidates[index];
      const shift = bestAutoShift(chart, referenceProfile);
      if (shift) shifts.push({ chart, shift });
      await visualizerLoopStep(
        index,
        candidates.length,
        3,
        stepTotal,
        "checking cosine similarity...",
      );
    }
    for (let index = 0; index < shifts.length; index += 1) {
      const { chart, shift } = shifts[index];
      setChartIntegrationByRt(chart, shift.rtStart, shift.rtEnd);
      chart.hasEdit = true;
      drawChart(chart);
      await visualizerLoopStep(
        index,
        shifts.length,
        3,
        stepTotal,
        "drawing shifted plots...",
      );
    }
    shiftedCount = shifts.length;
    if (shifts.length) {
      averageScore =
        shifts.reduce((sum, item) => sum + item.shift.score, 0) / shifts.length;
    }
  }

  const timestamp = new Date();
  const plan = numberedBackupPlan("reference", timestamp);
  await visualizerStep(4, stepTotal, "saving bounds and refreshing EchoMSkit outputs...");
  const result = await saveIntegrationEdits(
    visualizer.charts.filter((chart) => chart.hasEdit).map((chart) => ({
      sampleId: chart.sample.sampleId,
      wellLabel: chart.well.label,
      rtStart: chart.integration.rtStart,
      rtEnd: chart.integration.rtEnd,
    })),
    plan.name,
    plan.label,
    scrollY,
    shiftedCount
      ? `Applying reference offsets and auto-shifting ${shiftedCount.toLocaleString()} plot(s)...`
      : `Applying reference offsets across ${visualizer.charts.length.toLocaleString()} plot(s)...`,
    (saveResult) =>
      `Applied ${references.length.toLocaleString()} reference plot(s) to ${saveResult.written.toLocaleString()} EchoMSkit integration(s).`,
  );
  if (result) {
    visualizer.pendingSave = null;
    visualizerMessage(
      shiftedCount
        ? `Applied reference offsets, auto-shifted ${shiftedCount.toLocaleString()} plot(s), average cosine ${averageScore?.toFixed(3) ?? "-"}, and refreshed EchoMSkit outputs.`
        : `Applied reference offsets and refreshed EchoMSkit outputs.`,
    );
  }
}

// shifts non-reference plots toward the selected reference shape
async function autoShiftFromReferences() {
  if (!project?.path || running || visualizer.rendering) return;
  const references = visualizer.charts.filter((chart) => chart.isReference);
  if (!references.length) {
    showToast("Select at least one reference plot first.", "error");
    return;
  }
  await visualizerStep(1, 3, "building reference profile...");
  const referenceProfile = averageReferenceProfile(references);
  if (!referenceProfile) {
    showToast("The selected references do not have enough signal to build a profile.", "error");
    return;
  }
  await visualizerStep(2, 3, "checking plots with cosine similarity...");
  const candidates = visualizer.charts.filter((chart) => !chart.isReference);
  const shifts = [];
  for (let index = 0; index < candidates.length; index += 1) {
    const chart = candidates[index];
    const shift = bestAutoShift(chart, referenceProfile);
    if (shift) shifts.push({ chart, shift });
    await visualizerLoopStep(
      index,
      candidates.length,
      2,
      3,
      "checking cosine similarity...",
    );
  }
  if (!shifts.length) {
    visualizerMessage("Auto-shift found no plots that passed the similarity threshold.");
    showToast("No plots met the auto-shift threshold.");
    return;
  }

  const scrollY = window.scrollY;
  for (let index = 0; index < shifts.length; index += 1) {
    const { chart, shift } = shifts[index];
    setChartIntegrationByRt(chart, shift.rtStart, shift.rtEnd);
    chart.hasEdit = true;
    drawChart(chart);
    await visualizerLoopStep(
      index,
      shifts.length,
      2,
      3,
      "drawing shifted plots...",
    );
  }
  updateSaveButton();
  const timestamp = new Date();
  const shiftNumber = nextNumberedBackupNumber("autoshift");
  await visualizerStep(3, 3, "saving bounds and refreshing EchoMSkit outputs...");
  const result = await saveIntegrationEdits(
    visualizer.charts.filter((chart) => chart.hasEdit).map((chart) => ({
      sampleId: chart.sample.sampleId,
      wellLabel: chart.well.label,
      rtStart: chart.integration.rtStart,
      rtEnd: chart.integration.rtEnd,
    })),
    numberedBackupFileName("autoshift", shiftNumber, timestamp),
    `autoshift${shiftNumber}: ${displayTimestamp(timestamp)}`,
    scrollY,
    `Auto-shifting ${shifts.length.toLocaleString()} plot(s) with cosine profile matching...`,
    (saveResult) =>
      `Auto-shifted ${saveResult.written.toLocaleString()} EchoMSkit integration(s) from ${referenceProfile.count.toLocaleString()} reference plot(s).`,
  );
  if (result) {
    const averageScore =
      shifts.reduce((sum, item) => sum + item.shift.score, 0) / shifts.length;
    visualizerMessage(
      `Auto-shifted ${shifts.length.toLocaleString()} plot(s); average cosine ${averageScore.toFixed(3)}.`,
    );
  }
}

// restores the selected backup and reruns EchoMSkit
async function restoreSelectedBackup() {
  const name = elements.visualizerBackups.value;
  if (!project?.path || !name || running || visualizer.rendering) return;
  const scrollY = window.scrollY;
  visualizer.uniformView = null;
  visualizer.pendingSave = null;
  visualizerMessage(`Restoring ${backupLabel(name)} and refreshing EchoMSkit outputs...`);
  try {
    const result = isDesktop
      ? await invoke("visualizer_restore_bounds_backup", {
          projectPath: project.path,
          name,
        })
      : { backup: name, project };
    renderProject(result.project);
    addActivity(`Restored ${backupLabel(name)}.`, "success");
    showToast("Backup restored and EchoMSkit outputs refreshed.");
    await refreshBoundsBackups(name);
    await renderSelectedTransition({ scrollY });
  } catch (error) {
    visualizerMessage(String(error));
    addActivity(String(error), "error");
    showToast(String(error), "error");
  } finally {
    updateSaveButton();
    updateBackupButtons();
  }
}

// deletes a saved backup except original
async function deleteSelectedBackup() {
  const name = elements.visualizerBackups.value;
  if (!project?.path || !name || name === originalBackup || running || visualizer.rendering) {
    return;
  }
  if (
    !window.confirm(
      `Delete saved version "${backupLabel(name)}"?\n\nThis only deletes the backup file. Your current acq_time.csv will not be changed.`,
    )
  ) {
    return;
  }
  try {
    if (isDesktop) {
      await invoke("visualizer_delete_bounds_backup", {
        projectPath: project.path,
        name,
      });
    }
    addActivity(`Deleted ${backupLabel(name)}.`, "success");
    showToast("Backup deleted.");
    const selected = await refreshBoundsBackups();
    if (selected === originalBackup) {
      await restoreSelectedBackup();
    }
  } catch (error) {
    addActivity(String(error), "error");
    showToast(String(error), "error");
  }
}

// renames the selected backup label
async function renameSelectedBackup() {
  const name = elements.visualizerBackups.value;
  if (!project?.path || !name || name === originalBackup || running || visualizer.rendering) {
    return;
  }
  const label = await appPrompt(
    `Rename the selected acq_time.csv backup.\n\nFile: ${name}`,
    backupLabel(name),
    "Rename backup",
  );
  if (label == null) return;
  const trimmed = label.trim();
  if (!trimmed) {
    showToast("Backup label cannot be blank.", "error");
    return;
  }
  try {
    if (isDesktop) {
      await invoke("visualizer_rename_bounds_backup", {
        projectPath: project.path,
        name,
        label: trimmed,
      });
    } else {
      visualizer.backupLabels[name] = trimmed;
    }
    addActivity(`Renamed backup ${name}.`, "success");
    showToast("Backup renamed.");
    await refreshBoundsBackups(name);
  } catch (error) {
    addActivity(String(error), "error");
    showToast(String(error), "error");
  }
}

// fills the transition selector from backend data
function populateVisualizerTransitions() {
  const query = visualizer.search.trim().toLowerCase();
  const selected = elements.visualizerTransition.value;
  const fragment = document.createDocumentFragment();
  fragment.append(new Option("Select a transition", ""));
  let shown = 0;
  for (const transition of visualizer.transitions) {
    if (!query || transition.toLowerCase().includes(query)) {
      fragment.append(new Option(transition, transition));
      shown += 1;
    }
  }
  if (query && shown === 0) {
    fragment.append(new Option(`No matches for "${visualizer.search}"`, ""));
  }
  elements.visualizerTransition.replaceChildren(fragment);
  if ([...elements.visualizerTransition.options].some((option) => option.value === selected)) {
    elements.visualizerTransition.value = selected;
  }
}

// reads the active transition option
function selectedTransitionOption() {
  return elements.visualizerTransition.options[elements.visualizerTransition.selectedIndex] ?? null;
}

// previews highlighted transition text while minimized
function previewSelectedTransition() {
  const option = selectedTransitionOption();
  if (option?.value) {
    const message = `${option.textContent} selected. Press Enter to render.`;
    elements.visualizerAnalyte.textContent = message;
    visualizerMessage(message);
  } else {
    const message = "Choose a transition, then press Enter to render.";
    elements.visualizerAnalyte.textContent = message;
    visualizerMessage(message);
  }
}

// chooses a chart for keyboard zoom commands
function fallbackZoomChart() {
  if (visualizer.hoveredChart) return visualizer.hoveredChart;
  return visualizer.charts.find((chart) => {
    const rect = chart.canvas.getBoundingClientRect();
    return rect.bottom >= 0 && rect.top <= window.innerHeight;
  }) ?? visualizer.charts[0] ?? null;
}

// loads transition names and backup state
async function initializeVisualizer() {
  if (!project) {
    resetVisualizer();
    visualizerMessage("Select a processed dataset in Workflow first.");
    return;
  }
  elements.visualizerDataset.textContent = project.path;
  if (!project.outputs?.miscData) {
    resetVisualizer();
    elements.visualizerDataset.textContent = project.path;
    visualizerMessage("Run EchoMSkit first to create misc plot data.");
    return;
  }
  if (visualizer.projectPath === project.path && visualizer.transitions.length) {
    const projectPath = project.path;
    const projectToken = visualizer.projectToken;
    await refreshBoundsBackups();
    if (projectToken !== visualizer.projectToken || project?.path !== projectPath) return;
    visualizerMessage(`${visualizer.transitions.length.toLocaleString()} transitions ready`);
    return;
  }

  resetVisualizer();
  const projectPath = project.path;
  const projectToken = visualizer.projectToken;
  visualizer.projectPath = projectPath;
  elements.visualizerDataset.textContent = projectPath;
  visualizerMessage("Loading transition index...");
  try {
    const transitions = isDesktop
      ? await invoke("visualizer_list_transitions", { projectPath })
      : ["Mock transition", "Preview analyte"];
    if (projectToken !== visualizer.projectToken || project?.path !== projectPath) return;
    visualizer.transitions = transitions;
    populateVisualizerTransitions();
    await refreshBoundsBackups();
    if (projectToken !== visualizer.projectToken || project?.path !== projectPath) return;
    visualizerMessage(`${visualizer.transitions.length.toLocaleString()} transitions ready`);
  } catch (error) {
    if (projectToken !== visualizer.projectToken || project?.path !== projectPath) return;
    visualizerMessage(String(error));
    showToast(String(error), "error");
  }
}

// reads requested chart dimensions from controls
function visualizerDimensions() {
  return {
    width: Math.max(260, Math.min(1200, elements.visualizerWidth.valueAsNumber || 420)),
    height: Math.max(130, Math.min(700, elements.visualizerHeight.valueAsNumber || 190)),
  };
}

// returns drawable x-axis width
function plotWidth(chart) {
  return chart.width - chartMargins.left - chartMargins.right;
}

// returns drawable y-axis height
function plotHeight(chart) {
  return chart.height - chartMargins.top - chartMargins.bottom;
}

/* splits the time axis at the midpoints between neighbouring windows, so
adjacent plots meet without showing the same stretch of trace twice */
function wellLocalDomain(sample, well) {
  const points = sample.points;
  const bounds = (entry) => ({
    left: points[entry.startIndex]?.rt ?? entry.rtStart,
    right: points[entry.endIndex]?.rt ?? entry.rtEnd,
  });
  const index = sample.wells.indexOf(well);
  const { left, right } = bounds(well);
  const previous = index > 0 ? bounds(sample.wells[index - 1]).right : null;
  const next = index + 1 < sample.wells.length ? bounds(sample.wells[index + 1]).left : null;
  // overlapping neighbours keep the whole integration in view
  let before = previous === null ? null : Math.max((left - previous) / 2, 0);
  let after = next === null ? null : Math.max((next - right) / 2, 0);
  const fallback = Math.max((right - left) * 0.45, 0.03);
  before ??= after ?? fallback;
  after ??= before;
  const first = points[0]?.rt ?? left;
  const last = points[points.length - 1]?.rt ?? right;
  const start = Math.max(first, left - before);
  const end = Math.min(last, right + after);
  if (end <= start) {
    return { start: first, end: last > first ? last : first + 1 };
  }
  return { start, end };
}

// finds the strongest point inside a time window
function peakInWindow(points, rtStart, rtEnd) {
  let peak = null;
  for (const point of points) {
    if (point.rt < rtStart || point.rt > rtEnd) continue;
    if (!peak || point.intensity > peak.intensity) peak = point;
  }
  return peak;
}

// collects candidate local maxima for peak matching
function localPeakCandidates(points, rtStart, rtEnd, limit = 12) {
  const peaks = [];
  for (let index = 1; index < points.length - 1; index += 1) {
    const point = points[index];
    if (point.rt < rtStart || point.rt > rtEnd) continue;
    if (
      point.intensity >= points[index - 1].intensity &&
      point.intensity >= points[index + 1].intensity
    ) {
      peaks.push(point);
    }
  }
  const apex = peakInWindow(points, rtStart, rtEnd);
  if (apex && !peaks.some((point) => point.rt === apex.rt)) {
    peaks.push(apex);
  }
  return peaks
    .sort((left, right) => right.intensity - left.intensity)
    .slice(0, limit);
}

// finds the strongest signal in a displayed range
function maximumInRange(points, rtStart, rtEnd) {
  let max = 0;
  for (const point of points) {
    if (point.rt >= rtStart && point.rt <= rtEnd) {
      max = Math.max(max, point.intensity);
    }
  }
  return max;
}

// formats retention time for tooltips
function formatRt(value) {
  return value.toFixed(2);
}

// formats local x-axis labels
function formatAxisRt(value) {
  if (!Number.isFinite(value) || Math.abs(value) < 0.005) return "0";
  return value.toFixed(2);
}

// computes the local x-axis labels for the current view
function chartAxisLabels(chart) {
  const domain = visualizerDomain(chart);
  return {
    start: formatAxisRt(domain.rtStart),
    end: formatAxisRt(domain.rtEnd),
  };
}

// updates x-axis labels after zooming settles
function settleChartAxisLabels(chart, immediate = false) {
  if (!chart) return;
  window.clearTimeout(chart.axisLabelTimer);
  const applyLabels = () => {
    if (!visualizer.charts.includes(chart)) return;
    chart.axisLabels = chartAxisLabels(chart);
    drawChart(chart, chart.hoverIndex);
  };
  if (immediate) {
    applyLabels();
  } else {
    chart.axisLabelTimer = window.setTimeout(applyLabels, 500);
  }
}

// formats intensity for tooltips
function formatIntensity(value) {
  return Math.round(value).toLocaleString();
}

// formats one plotted point
function formatTooltipPoint(point) {
  return `${formatRt(point.rt)}, ${formatIntensity(point.intensity)}`;
}

// copies backend integration values into chart state
function integrationFromWell(well) {
  return {
    startIndex: well.startIndex,
    endIndex: well.endIndex,
    rtStart: well.rtStart,
    rtEnd: well.rtEnd,
    area: well.area,
  };
}

// recalculates area after dragged bounds change
function calculateArea(points, startIndex, endIndex) {
  let area = 0;
  for (let index = startIndex; index < endIndex; index += 1) {
    const left = points[index];
    const right = points[index + 1];
    area += (left.intensity + right.intensity) * (right.rt - left.rt) * 30;
  }
  return area;
}

// samples intensity between measured points
function interpolatedIntensity(points, rt) {
  if (!points.length) return 0;
  if (rt <= points[0].rt) return points[0].intensity;
  const last = points[points.length - 1];
  if (rt >= last.rt) return last.intensity;
  let low = 0;
  let high = points.length - 1;
  while (low < high) {
    const mid = Math.floor((low + high) / 2);
    if (points[mid].rt < rt) low = mid + 1;
    else high = mid;
  }
  const right = points[low];
  const left = points[low - 1] ?? right;
  const span = Math.max(right.rt - left.rt, Number.EPSILON);
  const amount = (rt - left.rt) / span;
  return left.intensity + (right.intensity - left.intensity) * amount;
}

// turns one mound into a comparable shape vector
function normalizedProfile(points, rtStart, rtEnd, count = autoShift.points) {
  if (!points.length || rtEnd <= rtStart || count < 2) return null;
  const first = points[0].rt;
  const last = points[points.length - 1].rt;
  if (rtStart < first || rtEnd > last) return null;
  const values = [];
  for (let index = 0; index < count; index += 1) {
    const amount = index / (count - 1);
    values.push(interpolatedIntensity(points, rtStart + (rtEnd - rtStart) * amount));
  }
  const baseline = Math.min(...values);
  const shifted = values.map((value) => Math.max(0, value - baseline));
  const signal = Math.max(...shifted);
  if (signal < autoShift.minSignal) return null;
  const norm = Math.sqrt(shifted.reduce((sum, value) => sum + value * value, 0));
  if (norm <= 0) return null;
  return {
    values: shifted.map((value) => value / norm),
    signal,
  };
}

// scores how similarly two mound shapes point
function cosineSimilarity(left, right) {
  if (!left || !right || left.length !== right.length) return 0;
  return left.reduce((sum, value, index) => sum + value * right[index], 0);
}

// averages selected reference mound shapes
function averageReferenceProfile(references) {
  const profiles = references
    .map((chart) => {
      const width = chart.integration.rtEnd - chart.integration.rtStart;
      const apex = peakInWindow(
        chart.sample.points,
        chart.integration.rtStart,
        chart.integration.rtEnd,
      );
      const apexFraction =
        apex && width > 0
          ? (apex.rt - chart.integration.rtStart) / width
          : 0.5;
      return {
        width,
        apexFraction,
        profile: normalizedProfile(
          chart.sample.points,
          chart.integration.rtStart,
          chart.integration.rtEnd,
        ),
      };
    })
    .filter((item) => item.width > 0 && item.profile && Number.isFinite(item.apexFraction));
  if (!profiles.length) return null;
  const values = Array.from({ length: autoShift.points }, (_, index) =>
    profiles.reduce((sum, item) => sum + item.profile.values[index], 0) / profiles.length,
  );
  const norm = Math.sqrt(values.reduce((sum, value) => sum + value * value, 0));
  if (norm <= 0) return null;
  return {
    values: values.map((value) => value / norm),
    width: profiles.reduce((sum, item) => sum + item.width, 0) / profiles.length,
    apexFraction: Math.max(
      0.12,
      Math.min(
        0.88,
        profiles.reduce((sum, item) => sum + item.apexFraction, 0) / profiles.length,
      ),
    ),
    count: profiles.length,
  };
}

// estimates the native spacing between scan points
function medianPointSpacing(points) {
  if (points.length < 2) return 0.005;
  const gaps = [];
  for (let index = 1; index < points.length; index += 1) {
    const gap = points[index].rt - points[index - 1].rt;
    if (gap > 0 && Number.isFinite(gap)) gaps.push(gap);
  }
  if (!gaps.length) return 0.005;
  gaps.sort((left, right) => left - right);
  return gaps[Math.floor(gaps.length / 2)];
}

// compares a nearby candidate window to the reference shape
function bestAutoShift(chart, referenceProfile) {
  const points = chart.sample.points;
  if (points.length < 3 || !referenceProfile?.values?.length) return null;
  const width = referenceProfile.width;
  const apexOffset = width * (referenceProfile.apexFraction ?? 0.5);
  const baseStart = chart.integration.rtStart;
  const first = points[0].rt;
  const last = points[points.length - 1].rt;
  const maxShift = Math.min(autoShift.maxShift, Math.max(width * 1.7, 0.08));
  const step = Math.max(0.002, Math.min(0.01, medianPointSpacing(points)));
  const searchStart = Math.max(first, baseStart - maxShift);
  const searchEnd = Math.min(last, baseStart + width + maxShift);
  const scoreAt = (start) => {
    const end = start + width;
    if (start < first || end > last) return null;
    const profile = normalizedProfile(points, start, end);
    if (!profile) return null;
    const peak = peakInWindow(points, start, end);
    if (!peak) return null;
    const apexFraction = (peak.rt - start) / width;
    if (
      Math.abs(apexFraction - referenceProfile.apexFraction) >
      autoShift.apexFractionTolerance
    ) {
      return null;
    }
    const leftEdge = interpolatedIntensity(points, start);
    const rightEdge = interpolatedIntensity(points, end);
    if (peak.intensity <= Math.max(leftEdge, rightEdge) * 1.08 + 1) return null;
    const shift = start - baseStart;
    const distancePenalty = maxShift > 0 ? Math.min(Math.abs(shift) / maxShift, 1) * 0.05 : 0;
    const score = cosineSimilarity(referenceProfile.values, profile.values);
    return {
      score,
      adjustedScore: score - distancePenalty,
    };
  };
  const baseCandidate = scoreAt(baseStart);
  const baseScore = baseCandidate?.score ?? 0;
  let best = {
    shift: 0,
    rtStart: baseStart,
    rtEnd: baseStart + width,
    score: baseScore,
    adjustedScore: baseCandidate?.adjustedScore ?? baseScore,
    baseScore,
  };
  const candidateStarts = [];
  for (let shift = -maxShift; shift <= maxShift + step / 2; shift += step) {
    candidateStarts.push(baseStart + shift);
  }
  for (const peak of localPeakCandidates(points, searchStart, searchEnd)) {
    const centeredStart = peak.rt - apexOffset;
    for (let offset = -step * 3; offset <= step * 3; offset += step) {
      candidateStarts.push(centeredStart + offset);
    }
  }
  for (const rtStart of uniqueSortedNumbers(candidateStarts)) {
    const candidate = scoreAt(rtStart);
    if (!candidate) continue;
    if (candidate.adjustedScore > best.adjustedScore) {
      best = {
        shift: rtStart - baseStart,
        rtStart,
        rtEnd: rtStart + width,
        score: candidate.score,
        adjustedScore: candidate.adjustedScore,
        baseScore,
      };
    }
  }
  if (
    Math.abs(best.shift) < step / 2 ||
    best.score < autoShift.minScore ||
    best.score - baseScore < autoShift.minImprovement
  ) {
    return null;
  }
  return best;
}

// removes duplicate candidate retention times
function uniqueSortedNumbers(values, precision = 5) {
  const seen = new Set();
  const output = [];
  for (const value of values) {
    if (!Number.isFinite(value)) continue;
    const key = value.toFixed(precision);
    if (seen.has(key)) continue;
    seen.add(key);
    output.push(value);
  }
  return output.sort((left, right) => left - right);
}

// sets chart bounds from two point indices
function setChartIntegration(chart, firstIndex, secondIndex) {
  const points = chart.sample.points;
  const startIndex = Math.max(0, Math.min(points.length - 2, Math.min(firstIndex, secondIndex)));
  const endIndex = Math.max(startIndex + 1, Math.min(points.length - 1, Math.max(firstIndex, secondIndex)));
  chart.integration = {
    startIndex,
    endIndex,
    rtStart: points[startIndex].rt,
    rtEnd: points[endIndex].rt,
    area: calculateArea(points, startIndex, endIndex),
  };
}

// sets chart bounds from retention times
function setChartIntegrationByRt(chart, rtStart, rtEnd) {
  const points = chart.sample.points;
  const startIndex = nearestPoint(points, Math.min(rtStart, rtEnd));
  const endIndex = nearestPoint(points, Math.max(rtStart, rtEnd));
  setChartIntegration(chart, startIndex, endIndex);
}

// compares saved and current bounds
function integrationsEqual(left, right) {
  if (!left || !right) return false;
  return (
    left.startIndex === right.startIndex &&
    left.endIndex === right.endIndex &&
    Math.abs(left.rtStart - right.rtStart) < 0.0005 &&
    Math.abs(left.rtEnd - right.rtEnd) < 0.0005
  );
}

// records whether a chart needs saving
function markChartDirty(chart) {
  chart.hasEdit = !integrationsEqual(chart.integration, chart.originalIntegration);
  updateSaveButton();
}

// gives each chart a stable reference key
function chartReferenceId(sample, well) {
  return `${sample.sampleId}::${well.label}`;
}

// labels each sample by its acquisition name
function sampleTitle(sample) {
  return sample.displayName || sample.sampleId;
}

// combines the acquisition and ejection window into the plot title
function chartTitle(sample, well) {
  const sampleId = typeof well.sampleId === "string" ? well.sampleId.trim() : "";
  const title = `${sampleTitle(sample)} ${well.label}`;
  return sampleId ? `${title} : ${sampleId}` : title;
}

// shows each reference's local start offset
function chartReferenceOffsetLabel(chart) {
  const domain = visualizerDomain(chart);
  const startOffset = chart.integration.rtStart - domain.rtStart;
  const endOffset = chart.integration.rtEnd - domain.rtStart;
  return `start +${startOffset.toFixed(3)}, end +${endOffset.toFixed(3)}`;
}

// syncs the selected reference styling
function syncChartReferenceState(chart) {
  chart.isReference = visualizer.referenceIds.has(chart.referenceId);
  chart.shell.classList.toggle("is-reference", chart.isReference);
  chart.referenceToggle.classList.toggle("is-selected", chart.isReference);
  chart.referenceToggle.setAttribute("aria-pressed", String(chart.isReference));
  chart.referenceToggle.title = chart.isReference
    ? `reference plot (${chartReferenceOffsetLabel(chart)})`
    : "select this plot as a reference";
}

// toggles a plot as a reference
function toggleChartReference(chart) {
  if (chart.isReference) {
    visualizer.referenceIds.delete(chart.referenceId);
  } else {
    visualizer.referenceIds.add(chart.referenceId);
  }
  syncChartReferenceState(chart);
  updateReferenceButton();
  visualizerMessage(
    `${visualizer.charts.filter((item) => item.isReference).length.toLocaleString()} reference plot(s) selected`,
  );
}

// chooses the first x and y window for a chart
function initialChartView(sample, well) {
  let rtStart = elements.visualizerRtStart.valueAsNumber || 0;
  let rtEnd = elements.visualizerRtEnd.valueAsNumber || 0;
  if (!visualizer.rangeManuallySet || rtEnd <= rtStart) {
    const local = wellLocalDomain(sample, well);
    rtStart = local.start;
    rtEnd = local.end;
  }
  const first = sample.points[0]?.rt ?? rtStart;
  const last = sample.points[sample.points.length - 1]?.rt ?? rtEnd;
  rtStart = Math.max(first, rtStart);
  rtEnd = Math.min(last, rtEnd > rtStart ? rtEnd : rtStart + 1);
  if (rtEnd <= rtStart) {
    rtStart = first;
    rtEnd = last > first ? last : first + 1;
  }
  let yMax = elements.visualizerIntensity.valueAsNumber || 0;
  if (yMax <= 0) {
    yMax =
      maximumInRange(sample.points, rtStart, rtEnd) ||
      well.height ||
      visualizer.data?.globalIntensityMax ||
      1;
    yMax *= 1.1;
  }
  yMax = Math.max(yMax, 1);
  return {
    baseRtStart: rtStart,
    baseRtEnd: rtEnd,
    rtStart,
    rtEnd,
    baseYMax: yMax,
    yMax,
  };
}

// combines chart view and y zoom into draw limits
function visualizerDomain(chart) {
  if (!chart.view) {
    chart.view = initialChartView(chart.sample, chart.well);
  }
  return {
    rtStart: chart.view.rtStart,
    rtEnd: chart.view.rtEnd > chart.view.rtStart ? chart.view.rtEnd : chart.view.rtStart + 1,
    yMax: Math.max(chart.view.yMax, 1),
  };
}

// updates the chart x-axis window
function setChartViewWindow(chart, rtStart, rtEnd) {
  if (!chart?.view) return;
  const first = chart.sample.points[0]?.rt ?? rtStart;
  const last = chart.sample.points[chart.sample.points.length - 1]?.rt ?? rtEnd;
  const totalSpan = Math.max(last - first, 0.002);
  let span = Math.max(rtEnd - rtStart, 0.002);
  let start = rtStart;
  let end = rtEnd;
  if (span >= totalSpan) {
    start = first;
    end = last;
    span = totalSpan;
  } else {
    if (start < first) {
      start = first;
      end = start + span;
    }
    if (end > last) {
      end = last;
      start = end - span;
    }
  }
  chart.view.baseRtStart = Math.min(chart.view.baseRtStart, start);
  chart.view.baseRtEnd = Math.max(chart.view.baseRtEnd, end);
  chart.view.rtStart = Math.max(first, start);
  chart.view.rtEnd = Math.min(last, end);
  settleChartAxisLabels(chart);
  const manualYMax = elements.visualizerIntensity.valueAsNumber || 0;
  if (manualYMax > 0) {
    chart.view.baseYMax = manualYMax;
    chart.view.yMax = manualYMax;
  } else {
    chart.view.baseYMax = Math.max(
      (maximumInRange(chart.sample.points, chart.view.rtStart, chart.view.rtEnd) ||
        chart.well.height ||
        visualizer.data?.globalIntensityMax ||
        1) * 1.1,
      1,
    );
    chart.view.yMax = chart.view.baseYMax;
  }
}

// reuses the same relative view after uniform reference apply
function applyUniformViewToChart(chart) {
  const uniform = visualizer.uniformView;
  if (!uniform || uniform.transition !== visualizer.data?.transition) return;
  const viewStart = chart.integration.rtStart - uniform.startOffset;
  setChartViewWindow(chart, viewStart, viewStart + uniform.viewSpan);
}

// syncs the horizontal pan slider
function updatePanSlider(chart) {
  if (!chart.panSlider || !chart.view) return;
  const baseSpan = chart.view.baseRtEnd - chart.view.baseRtStart;
  const viewSpan = chart.view.rtEnd - chart.view.rtStart;
  if (baseSpan <= 0 || viewSpan >= baseSpan * 0.999) {
    chart.panSlider.disabled = true;
    chart.panSlider.value = "0";
    return;
  }
  const span = Math.max(baseSpan - viewSpan, Number.EPSILON);
  const value = ((chart.view.rtStart - chart.view.baseRtStart) / span) * 1000;
  chart.panSlider.disabled = false;
  chart.panSlider.value = String(Math.max(0, Math.min(1000, Math.round(value))));
}

// syncs the vertical zoom slider
function updateYSlider(chart) {
  if (!chart.ySlider || !chart.view) return;
  const amount = Math.log(chart.view.yMax / chart.view.baseYMax) / Math.log(0.08);
  chart.ySlider.value = String(Math.max(0, Math.min(1000, Math.round(amount * 1000))));
}

// redraws one chart with the latest hover state
function redrawChart(chart, hoverIndex = null) {
  updatePanSlider(chart);
  updateYSlider(chart);
  drawChart(chart, hoverIndex);
}

// converts mouse x-position into retention time
function chartRtAtClientX(chart, clientX) {
  const rect = chart.canvas.getBoundingClientRect();
  const domain = visualizerDomain(chart);
  const localX = (clientX - rect.left) * chart.width / (rect.width || chart.width);
  const x = Math.max(
    chartMargins.left,
    Math.min(chart.width - chartMargins.right, localX),
  );
  return (
    domain.rtStart +
    ((x - chartMargins.left) / Math.max(plotWidth(chart), 1)) *
      (domain.rtEnd - domain.rtStart)
  );
}

// finds the nearest point under the cursor
function chartPointIndexAtClientX(chart, clientX) {
  return nearestPoint(chart.sample.points, chartRtAtClientX(chart, clientX));
}

// checks whether the pointer is inside plot bounds
function chartPointerInsidePlot(chart, event) {
  const rect = chart.canvas.getBoundingClientRect();
  const x = (event.clientX - rect.left) * chart.width / (rect.width || chart.width);
  const y = (event.clientY - rect.top) * chart.height / (rect.height || chart.height);
  return (
    x >= chartMargins.left &&
    x <= chart.width - chartMargins.right &&
    y >= chartMargins.top &&
    y <= chart.height - chartMargins.bottom
  );
}

// zooms horizontally around the cursor when possible
function zoomChartHorizontal(chart, factor, anchorClientX = null) {
  if (!chart?.view) return;
  const baseSpan = chart.view.baseRtEnd - chart.view.baseRtStart;
  const currentSpan = chart.view.rtEnd - chart.view.rtStart;
  if (baseSpan <= 0 || currentSpan <= 0) return;
  const minSpan = Math.max(baseSpan / 1000, 0.002);
  const nextSpan = Math.max(minSpan, Math.min(baseSpan, currentSpan / factor));
  let anchorRt =
    anchorClientX == null
      ? (chart.view.rtStart + chart.view.rtEnd) / 2
      : chartRtAtClientX(chart, anchorClientX);
  anchorRt = Math.max(chart.view.rtStart, Math.min(chart.view.rtEnd, anchorRt));
  const ratio = (anchorRt - chart.view.rtStart) / currentSpan;
  let rtStart = anchorRt - nextSpan * ratio;
  let rtEnd = rtStart + nextSpan;
  if (rtStart < chart.view.baseRtStart) {
    rtStart = chart.view.baseRtStart;
    rtEnd = rtStart + nextSpan;
  }
  if (rtEnd > chart.view.baseRtEnd) {
    rtEnd = chart.view.baseRtEnd;
    rtStart = rtEnd - nextSpan;
  }
  chart.view.rtStart = Math.max(chart.view.baseRtStart, rtStart);
  chart.view.rtEnd = Math.min(chart.view.baseRtEnd, rtEnd);
  redrawChart(chart);
}

// applies vertical zoom from the slider
function setChartYAmount(chart, amount) {
  if (!chart?.view) return;
  const clamped = Math.max(0, Math.min(1, amount));
  chart.view.yMax = Math.max(chart.view.baseYMax * Math.pow(0.08, clamped), 1);
  redrawChart(chart);
}

// pans the x-axis from the horizontal slider
function panChartToSlider(chart) {
  if (!chart?.view || !chart.panSlider || chart.panSlider.disabled) return;
  const baseSpan = chart.view.baseRtEnd - chart.view.baseRtStart;
  const viewSpan = chart.view.rtEnd - chart.view.rtStart;
  const offset = (Number(chart.panSlider.value) / 1000) * Math.max(0, baseSpan - viewSpan);
  chart.view.rtStart = chart.view.baseRtStart + offset;
  chart.view.rtEnd = chart.view.rtStart + viewSpan;
  drawChart(chart);
}

// reads resolved css custom properties
function cssColor(name) {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

// caches css colors between redraws
function chartColors() {
  if (!visualizer.chartColors) {
    visualizer.chartColors = {
      ink: cssColor("--ink"),
      muted: cssColor("--muted"),
      line: cssColor("--line"),
      blue: cssColor("--blue"),
      green: cssColor("--green"),
      surface: cssColor("--surface"),
      areaOpacity: Number(cssColor("--plot-integration-area-opacity")) || 0.52,
      windowOpacity: Number(cssColor("--plot-integration-window-opacity")) || 0.16,
    };
  }
  return visualizer.chartColors;
}

// finds the closest measured point by rt
function nearestPoint(points, rt) {
  let low = 0;
  let high = points.length - 1;
  while (low < high) {
    const mid = Math.floor((low + high) / 2);
    if (points[mid].rt < rt) low = mid + 1;
    else high = mid;
  }
  if (low > 0 && Math.abs(points[low - 1].rt - rt) < Math.abs(points[low].rt - rt)) {
    return low - 1;
  }
  return low;
}

// binary-searches the first visible point
function firstPointAtOrAfter(points, rt) {
  let low = 0;
  let high = points.length;
  while (low < high) {
    const mid = Math.floor((low + high) / 2);
    if (points[mid].rt < rt) low = mid + 1;
    else high = mid;
  }
  return low;
}

// draws the small tooltip box
function roundedRect(ctx, x, y, width, height, radius = 5) {
  const safeRadius = Math.min(radius, width / 2, height / 2);
  ctx.beginPath();
  ctx.moveTo(x + safeRadius, y);
  ctx.lineTo(x + width - safeRadius, y);
  ctx.quadraticCurveTo(x + width, y, x + width, y + safeRadius);
  ctx.lineTo(x + width, y + height - safeRadius);
  ctx.quadraticCurveTo(x + width, y + height, x + width - safeRadius, y + height);
  ctx.lineTo(x + safeRadius, y + height);
  ctx.quadraticCurveTo(x, y + height, x, y + height - safeRadius);
  ctx.lineTo(x, y + safeRadius);
  ctx.quadraticCurveTo(x, y, x + safeRadius, y);
  ctx.closePath();
}

// paints one chromatogram canvas
function drawChart(chart, hoverIndex = null, tooltipText = null) {
  const { canvas, sample, well } = chart;
  const { width, height } = chart;
  const ratio = Math.min(window.devicePixelRatio || 1, 2);
  const pixelWidth = Math.floor(width * ratio);
  const pixelHeight = Math.floor(height * ratio);
  if (canvas.width !== pixelWidth || canvas.height !== pixelHeight) {
    canvas.width = pixelWidth;
    canvas.height = pixelHeight;
    canvas.style.width = `${width}px`;
    canvas.style.height = `${height}px`;
  }
  const ctx = canvas.getContext("2d");
  ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
  ctx.clearRect(0, 0, width, height);

  const { ink, muted, line, blue, green, surface, areaOpacity, windowOpacity } = chartColors();
  const margins = chartMargins;
  const plotWidth = width - margins.left - margins.right;
  const plotHeight = height - margins.top - margins.bottom;
  const domain = visualizerDomain(chart);
  const points = sample.points;
  const integration = chart.integration ?? integrationFromWell(well);
  const x = (rt) => margins.left + ((rt - domain.rtStart) / (domain.rtEnd - domain.rtStart)) * plotWidth;
  const y = (intensity) => margins.top + plotHeight - Math.min(intensity, domain.yMax) / domain.yMax * plotHeight;
  const plotLeft = margins.left;
  const plotRight = width - margins.right;
  const plotTop = margins.top;
  const plotBottom = margins.top + plotHeight;
  const clipPlot = () => {
    ctx.beginPath();
    ctx.rect(plotLeft, plotTop, plotWidth, plotHeight);
    ctx.clip();
  };

  ctx.fillStyle = surface;
  ctx.fillRect(0, 0, width, height);
  ctx.strokeStyle = line;
  ctx.lineWidth = 1;
  ctx.globalAlpha = 0.55;
  ctx.beginPath();
  ctx.moveTo(margins.left, margins.top);
  ctx.lineTo(margins.left, plotBottom);
  ctx.moveTo(margins.left, plotBottom);
  ctx.lineTo(plotRight, plotBottom);
  ctx.stroke();
  ctx.globalAlpha = 1;

  ctx.save();
  ctx.translate(10, margins.top + plotHeight / 2);
  ctx.rotate(-Math.PI / 2);
  ctx.fillStyle = muted;
  ctx.font = "9px -apple-system, BlinkMacSystemFont, Segoe UI, sans-serif";
  ctx.textAlign = "center";
  ctx.fillText("intensity", 0, 0);
  ctx.restore();

  ctx.strokeStyle = line;
  ctx.lineWidth = 1;
  ctx.globalAlpha = 0.3;
  ctx.beginPath();
  ctx.moveTo(margins.left, plotBottom);
  ctx.lineTo(plotRight, plotBottom);
  ctx.stroke();
  ctx.globalAlpha = 1;

  const windowLeft = x(integration.rtStart);
  const windowRight = x(integration.rtEnd);
  const shadeLeft = Math.max(plotLeft, Math.min(plotRight, Math.min(windowLeft, windowRight)));
  const shadeRight = Math.max(plotLeft, Math.min(plotRight, Math.max(windowLeft, windowRight)));
  ctx.fillStyle = "rgb(71, 124, 147)";
  ctx.globalAlpha = windowOpacity;
  if (shadeRight > shadeLeft) {
    ctx.fillRect(shadeLeft, margins.top, shadeRight - shadeLeft, plotHeight);
  }
  ctx.globalAlpha = 1;

  ctx.save();
  clipPlot();
  const start = Math.max(0, integration.startIndex);
  const end = Math.min(points.length - 1, integration.endIndex);
  if (end > start) {
    ctx.beginPath();
    ctx.moveTo(x(points[start].rt), y(0));
    for (let index = start; index <= end; index += 1) {
      const point = points[index];
      ctx.lineTo(x(point.rt), y(point.intensity));
    }
    ctx.lineTo(x(points[end].rt), y(0));
    ctx.closePath();
    ctx.fillStyle = "rgb(95, 147, 105)";
    ctx.globalAlpha = areaOpacity;
    ctx.fill();
    ctx.globalAlpha = 1;
  }

  ctx.beginPath();
  let started = false;
  const visibleStart = firstPointAtOrAfter(points, domain.rtStart);
  let visibleEnd = firstPointAtOrAfter(points, domain.rtEnd);
  while (visibleEnd < points.length && points[visibleEnd].rt <= domain.rtEnd) {
    visibleEnd += 1;
  }
  // one point past each edge so the clipped trace runs into the neighbouring plot
  const traceEnd = Math.min(points.length, visibleEnd + 1);
  for (let index = Math.max(0, visibleStart - 1); index < traceEnd; index += 1) {
    const point = points[index];
    const px = x(point.rt);
    const py = y(point.intensity);
    if (!started) {
      ctx.moveTo(px, py);
      started = true;
    } else {
      ctx.lineTo(px, py);
    }
  }
  ctx.strokeStyle = ink;
  ctx.lineWidth = 1.25;
  ctx.lineJoin = "round";
  ctx.lineCap = "round";
  ctx.stroke();

  ctx.fillStyle = ink;
  ctx.globalAlpha = 0.5;
  const visibleCount = Math.max(0, visibleEnd - visibleStart);
  const dotEvery = Math.max(1, Math.ceil(visibleCount / 420));
  let visibleIndex = 0;
  for (let index = visibleStart; index < visibleEnd; index += 1) {
    const point = points[index];
    if (visibleIndex % dotEvery !== 0) {
      visibleIndex += 1;
      continue;
    }
    visibleIndex += 1;
    ctx.beginPath();
    ctx.arc(x(point.rt), y(point.intensity), 1.45, 0, Math.PI * 2);
    ctx.fill();
  }
  ctx.globalAlpha = 1;

  let tooltip = null;
  if (hoverIndex != null && points[hoverIndex]) {
    const point = points[hoverIndex];
    const px = x(point.rt);
    const py = y(point.intensity);
    if (px >= margins.left && px <= width - margins.right) {
      ctx.strokeStyle = blue;
      ctx.beginPath();
      ctx.moveTo(px, margins.top);
      ctx.lineTo(px, margins.top + plotHeight);
      ctx.stroke();
      ctx.fillStyle = blue;
      ctx.beginPath();
      ctx.arc(px, py, 3.5, 0, Math.PI * 2);
      ctx.fill();
      tooltip = {
        text: tooltipText ?? `(${formatTooltipPoint(point)})`,
        px,
        py,
      };
    }
  }
  ctx.restore();

  ctx.fillStyle = ink;
  ctx.font = "700 10px -apple-system, BlinkMacSystemFont, Segoe UI, sans-serif";
  ctx.textAlign = "center";
  ctx.fillText(chartTitle(sample, well), width / 2, 12);
  ctx.fillStyle = muted;
  ctx.font = "10px ui-monospace, Consolas, monospace";
  const axisLabels = chart.axisLabels ?? chartAxisLabels(chart);
  ctx.textAlign = "left";
  ctx.fillText(axisLabels.start, margins.left, height - 5);
  ctx.textAlign = "right";
  ctx.fillText(axisLabels.end, width - margins.right, height - 5);
  ctx.fillText("0", margins.left - 4, plotBottom - 2);
  ctx.fillText(`${Math.round(domain.yMax).toLocaleString()}`, margins.left - 4, margins.top + 4);
  ctx.fillStyle = green;
  ctx.fillText(`area ${Math.round(integration.area).toLocaleString()}`, width - 10, 12);
  ctx.textAlign = "left";

  if (tooltip) {
    ctx.font = "10px ui-monospace, Consolas, monospace";
    ctx.textAlign = "center";
    const padding = 8;
    const boxWidth = Math.min(220, Math.max(72, ctx.measureText(tooltip.text).width + padding * 2));
    const boxHeight = 20;
    const boxX = Math.min(
      plotRight - boxWidth - 4,
      Math.max(plotLeft + 4, tooltip.px - boxWidth / 2),
    );
    const boxY = Math.min(
      plotBottom - boxHeight - 4,
      Math.max(plotTop + 4, tooltip.py - boxHeight - 8),
    );
    ctx.fillStyle = ink;
    roundedRect(ctx, boxX, boxY, boxWidth, boxHeight, 3);
    ctx.fill();
    ctx.fillStyle = surface;
    ctx.fillText(tooltip.text, boxX + boxWidth / 2, boxY + 13);
  }
}

// builds one interactive plot shell
function createChart(sample, well, index) {
  const { width, height } = visualizerDimensions();
  const shell = document.createElement("article");
  shell.className = "visualizer-chart-shell";
  const frame = document.createElement("div");
  frame.className = "visualizer-canvas-frame";
  frame.style.width = `${width}px`;
  frame.style.height = `${height}px`;
  const canvas = document.createElement("canvas");
  canvas.className = "visualizer-chart";
  const referenceToggle = document.createElement("button");
  referenceToggle.className = "reference-toggle";
  referenceToggle.type = "button";
  referenceToggle.setAttribute("aria-pressed", "false");
  referenceToggle.setAttribute("aria-label", `select ${chartTitle(sample, well)} as a reference plot`);
  referenceToggle.innerHTML = `
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d="M5 12.5 10 17l9-10" />
    </svg>
  `;
  const ySlider = document.createElement("input");
  ySlider.className = "chart-y-slider";
  ySlider.type = "range";
  ySlider.min = "0";
  ySlider.max = "1000";
  ySlider.step = "1";
  ySlider.value = "0";
  ySlider.title = "adjust the y-axis scale";
  ySlider.setAttribute("aria-label", `adjust y-axis scale for ${chartTitle(sample, well)}`);
  frame.append(canvas, referenceToggle, ySlider);
  const panSlider = document.createElement("input");
  panSlider.className = "chart-pan-slider";
  panSlider.type = "range";
  panSlider.min = "0";
  panSlider.max = "1000";
  panSlider.step = "1";
  panSlider.value = "0";
  panSlider.disabled = true;
  panSlider.title = "pan the zoomed graph left or right";
  panSlider.setAttribute("aria-label", `pan zoomed graph for ${chartTitle(sample, well)}`);
  shell.append(frame, panSlider);
  elements.visualizerPlots.append(shell);
  const originalIntegration = integrationFromWell(well);
  const chart = {
    shell,
    canvas,
    frame,
    referenceToggle,
    panSlider,
    ySlider,
    sample,
    well,
    index,
    width,
    height,
    view: initialChartView(sample, well),
    axisLabels: null,
    axisLabelTimer: null,
    hoverX: null,
    hoverIndex: null,
    hoverInside: false,
    referenceId: chartReferenceId(sample, well),
    isReference: false,
    originalIntegration,
    integration: { ...originalIntegration },
    hasEdit: false,
  };
  syncChartReferenceState(chart);
  referenceToggle.addEventListener("click", (event) => {
    event.stopPropagation();
    toggleChartReference(chart);
  });
  applyUniformViewToChart(chart);
  syncChartReferenceState(chart);
  settleChartAxisLabels(chart, true);
  let dragging = false;
  let dragStartIndex = 0;
  let dragStartClientX = 0;
  let preDragIntegration = null;
  const dragTooltip = (pointIndex) => {
    const startPoint = sample.points[dragStartIndex];
    const point = sample.points[pointIndex];
    return `(${formatTooltipPoint(startPoint)}) -> (${formatTooltipPoint(point)})`;
  };
  canvas.addEventListener("pointerdown", (event) => {
    if (event.button !== 0 || !chartPointerInsidePlot(chart, event)) return;
    dragging = true;
    visualizer.hoveredChart = chart;
    chart.hoverX = event.clientX;
    dragStartClientX = event.clientX;
    dragStartIndex = chartPointIndexAtClientX(chart, event.clientX);
    preDragIntegration = { ...chart.integration };
    canvas.classList.add("is-dragging");
    drawChart(chart, dragStartIndex, dragTooltip(dragStartIndex));
    try {
      canvas.setPointerCapture(event.pointerId);
    } catch {
      // pointer capture is best-effort in webview
    }
    event.preventDefault();
  });
  canvas.addEventListener("pointermove", (event) => {
    visualizer.hoveredChart = chart;
    const insidePlot = chartPointerInsidePlot(chart, event);
    chart.hoverX = insidePlot ? event.clientX : null;
    const pointIndex = chartPointIndexAtClientX(chart, event.clientX);
    const point = sample.points[pointIndex];
    if (!dragging && chart.hoverIndex === pointIndex && chart.hoverInside === insidePlot) {
      return;
    }
    chart.hoverIndex = pointIndex;
    chart.hoverInside = insidePlot;
    if (dragging) {
      setChartIntegration(chart, dragStartIndex, pointIndex);
      drawChart(chart, pointIndex, dragTooltip(pointIndex));
    } else {
      drawChart(chart, pointIndex);
    }
    visualizerMessage(
      `${chartTitle(sample, well)}  RT ${point.rt.toFixed(3)}  intensity ${Math.round(point.intensity).toLocaleString()}  apex ${well.apexRt.toFixed(3)}`,
    );
  });
  const finishDrag = (event) => {
    if (!dragging) return;
    const pointIndex = chartPointIndexAtClientX(chart, event.clientX);
    if (Math.abs(event.clientX - dragStartClientX) < 4) {
      chart.integration = { ...preDragIntegration };
    } else {
      setChartIntegration(chart, dragStartIndex, pointIndex);
    }
    markChartDirty(chart);
    syncChartReferenceState(chart);
    chart.hoverIndex = pointIndex;
    chart.hoverInside = chartPointerInsidePlot(chart, event);
    dragging = false;
    preDragIntegration = null;
    canvas.classList.remove("is-dragging");
    try {
      canvas.releasePointerCapture(event.pointerId);
    } catch {
      // pointer capture is best-effort in webview
    }
    drawChart(chart, pointIndex);
  };
  canvas.addEventListener("pointerup", finishDrag);
  canvas.addEventListener("pointercancel", finishDrag);
  canvas.addEventListener("pointerleave", () => {
    if (dragging) return;
    if (visualizer.hoveredChart === chart) {
      visualizer.hoveredChart = null;
    }
    chart.hoverIndex = null;
    chart.hoverInside = false;
    drawChart(chart);
    if (visualizer.data) {
      visualizerMessage(
        `${visualizer.data.wellCount.toLocaleString()} well plots rendered`,
      );
    }
  });
  canvas.addEventListener(
    "wheel",
    (event) => {
      event.preventDefault();
      visualizer.hoveredChart = chart;
      chart.hoverX = chartPointerInsidePlot(chart, event) ? event.clientX : null;
      zoomChartHorizontal(chart, event.deltaY < 0 ? 1.25 : 0.8, chart.hoverX);
    },
    { passive: false },
  );
  panSlider.addEventListener("input", () => {
    panChartToSlider(chart);
  });
  ySlider.addEventListener("input", () => {
    setChartYAmount(chart, Number(ySlider.value) / 1000);
  });
  ySlider.addEventListener(
    "wheel",
    (event) => {
      event.preventDefault();
      const delta = event.deltaY < 0 ? 36 : -36;
      ySlider.value = String(
        Math.max(0, Math.min(1000, Number(ySlider.value) + delta)),
      );
      setChartYAmount(chart, Number(ySlider.value) / 1000);
    },
    { passive: false },
  );
  redrawChart(chart);
  return chart;
}

// stops chunked chart creation
function cancelVisualizerRendering() {
  if (!visualizer.rendering) return;
  visualizer.renderToken += 1;
  visualizer.rendering = false;
  elements.visualizerCancel.classList.add("hidden");
  elements.visualizerTransition.disabled = false;
  elements.visualizerRefresh.disabled = false;
  updateSaveButton();
  updateBackupButtons();
  visualizerMessage(`Rendering cancelled. ${visualizer.charts.length.toLocaleString()} plots shown.`);
}

// renders plots in small batches for smoother scrolling
function renderVisualizerCharts(data, options = {}) {
  visualizer.renderToken += 1;
  const token = visualizer.renderToken;
  const scrollY = options.scrollY ?? null;
  if (!options.keepUniformView) {
    visualizer.uniformView = null;
  }
  visualizer.data = data;
  visualizer.charts = [];
  visualizer.hoveredChart = null;
  visualizer.pendingSave = null;
  visualizer.rendering = true;
  updateSaveButton();
  updateBackupButtons();
  elements.visualizerPlots.replaceChildren();
  elements.visualizerView.classList.add("has-plots");
  elements.visualizerEmpty.classList.add("hidden");
  elements.visualizerCancel.classList.remove("hidden");
  elements.visualizerTransition.disabled = true;
  elements.visualizerRefresh.disabled = true;
  elements.visualizerAnalyte.textContent = data.transition;
  const jobs = [];
  for (const sample of data.samples) {
    for (const well of sample.wells) {
      if (
        Number.isFinite(well.height) &&
        Number.isFinite(well.area) &&
        well.height > 0 &&
        well.area > 0 &&
        well.rtEnd > well.rtStart &&
        well.endIndex > well.startIndex
      ) {
        jobs.push({ sample, well });
      }
    }
  }
  let cursor = 0;
  if (jobs.length === 0) {
    visualizer.rendering = false;
    elements.visualizerCancel.classList.add("hidden");
    elements.visualizerTransition.disabled = false;
    elements.visualizerRefresh.disabled = false;
    updateSaveButton();
    updateBackupButtons();
    visualizerMessage("No nonblank well plots were found for this transition.");
    restoreVisualizerScroll(scrollY);
    return;
  }
  visualizerMessage(`Rendering 0 / ${jobs.length.toLocaleString()} plots...`);
  const drawBatch = () => {
    if (token !== visualizer.renderToken) return;
    const deadline = performance.now() + 12;
    while (cursor < jobs.length && performance.now() < deadline) {
      const job = jobs[cursor];
      visualizer.charts.push(createChart(job.sample, job.well, cursor));
      cursor += 1;
    }
    if (cursor < jobs.length) {
      visualizerMessage(`Rendering ${cursor.toLocaleString()} / ${jobs.length.toLocaleString()} plots...`);
      requestAnimationFrame(drawBatch);
    } else {
      visualizer.rendering = false;
      elements.visualizerCancel.classList.add("hidden");
      elements.visualizerTransition.disabled = false;
      elements.visualizerRefresh.disabled = false;
      updateSaveButton();
      updateBackupButtons();
      visualizerMessage(`${jobs.length.toLocaleString()} well plots rendered`);
      restoreVisualizerScroll(scrollY);
    }
  };
  requestAnimationFrame(drawBatch);
}

// loads and renders the selected transition
async function renderSelectedTransition(options = {}) {
  const transition = elements.visualizerTransition.value;
  if (!transition || visualizer.rendering) return;
  const scrollY =
    options.scrollY ?? (options.preserveScroll ? window.scrollY : null);
  if (!project?.outputs?.miscData) {
    visualizerMessage("Run EchoMSkit first to create misc plot data.");
    return;
  }
  cancelVisualizerRendering();
  const token = ++visualizer.loadToken;
  const projectPath = project.path;
  elements.visualizerPlots.replaceChildren();
  visualizerMessage("Loading transition data...");
  try {
    const data = isDesktop
      ? await invoke("visualizer_transition", {
          projectPath,
          transition,
        })
      : { ...mockVisualizerData, transition };
    if (token !== visualizer.loadToken || project?.path !== projectPath) return;
    renderVisualizerCharts(data, {
      scrollY,
      keepUniformView: options.keepUniformView === true,
    });
  } catch (error) {
    if (token !== visualizer.loadToken || project?.path !== projectPath) return;
    visualizerMessage(String(error));
    showToast(String(error), "error");
  }
}

// updates presentation without replacing unsaved integration edits
function redrawVisualizerCharts(options = {}) {
  if (!visualizer.data || visualizer.rendering) return;
  const { width, height } = visualizerDimensions();
  for (const chart of visualizer.charts) {
    chart.width = width;
    chart.height = height;
    chart.frame.style.width = `${width}px`;
    chart.frame.style.height = `${height}px`;
    if (options.resetView === true) {
      chart.view = initialChartView(chart.sample, chart.well);
      settleChartAxisLabels(chart, true);
    }
    redrawChart(chart);
  }
}

const guiScales = new Set([90, 100, 110, 125, 150, 175, 200]);
const guiWidthBreakpoints = [1100, 850, 680, 620];

// css zoom leaves media queries on the unscaled viewport, so the scaled
// width tiers and height are exposed for the responsive layouts instead
function updateGuiScaleLayout(scale) {
  const factor = scale / 100;
  const effectiveWidth = window.innerWidth / factor;
  document.documentElement.dataset.guiWidth = guiWidthBreakpoints
    .filter((breakpoint) => effectiveWidth <= breakpoint)
    .join(" ");
  document.documentElement.style.setProperty(
    "--viewport-height",
    `${window.innerHeight / factor}px`,
  );
}

// scales the complete interface and remembers the accessibility preference
function applyGuiScale(value, persist = true) {
  const requestedScale = Number(value);
  const scale = guiScales.has(requestedScale) ? requestedScale : 100;
  document.documentElement.style.zoom = String(scale / 100);
  updateGuiScaleLayout(scale);
  elements.guiScale.value = String(scale);
  if (persist) {
    try {
      localStorage.setItem("echomskit-gui-scale", String(scale));
    } catch {
      // the selected scale still applies for this session
    }
  }
  window.requestAnimationFrame(redrawVisualizerCharts);
}

// keeps the theme button label accurate
function updateThemeToggle() {
  const isDark = document.documentElement.dataset.theme === "dark";
  elements.themeToggle.setAttribute(
    "aria-label",
    isDark ? "disable night mode" : "enable night mode",
  );
  elements.themeToggle.title = isDark
    ? "disable night mode"
    : "enable night mode";
}

// toggles light and dark mode
async function toggleTheme() {
  const theme =
    document.documentElement.dataset.theme === "dark" ? "light" : "dark";
  document.documentElement.dataset.theme = theme;
  visualizer.chartColors = null;
  try {
    localStorage.setItem("echomskit-theme", theme);
  } catch {
    // the selected theme still applies for this session
  }
  if (isDesktop) {
    try {
      await invoke("set_theme", { theme });
    } catch (error) {
      showToast(String(error), "error");
    }
  }
  updateThemeToggle();
  for (const chart of visualizer.charts) drawChart(chart);
}

// opens the slinghub link from the footer
async function openExternalSling(event) {
  if (!isDesktop) return;
  event.preventDefault();
  try {
    await invoke("open_sling");
  } catch (error) {
    showToast(String(error), "error");
  }
}

// subscribes to tauri worker events
async function registerDesktopEvents() {
  if (!isDesktop) return;
  await tauri.event.listen("worker-output", ({ payload }) => {
    addActivity(payload.line, payload.stream === "error" ? "error" : "");
  });
  await tauri.event.listen("run-state", ({ payload }) => {
    setRunning(payload.status === "running");
  });
}

// wires the gui once the page is ready
async function bootstrap() {
  const initialScale =
    Math.round((Number.parseFloat(document.documentElement.style.zoom) || 1) * 100);
  applyGuiScale(initialScale, false);
  for (const button of elements.chooseButtons) {
    button.addEventListener("click", chooseProject);
  }
  elements.runButton.addEventListener("click", runAll);
  elements.runPanel.addEventListener("click", (event) => {
    if (!event.target.closest("button")) elements.runButton.click();
  });
  elements.editDataButton.addEventListener("click", openDataEditor);
  elements.dataEditorClose.addEventListener("click", closeDataEditor);
  elements.dataEditorSave.addEventListener("click", saveDataEditorFile);
  elements.dataEditorRefresh.addEventListener("click", refreshDataEditorFile);
  elements.dataEditorRestore.addEventListener("click", restoreDataEditorBackup);
  elements.dataEditorRename.addEventListener("click", renameDataEditorBackup);
  elements.dataEditorDelete.addEventListener("click", deleteDataEditorBackup);
  elements.dataEditorAddRow.addEventListener("click", addDataEditorRow);
  elements.dataEditorAddColumn.addEventListener("click", addDataEditorColumn);
  elements.dataEditorBackups.addEventListener("change", () =>
    openDataEditorBackup(elements.dataEditorBackups.value),
  );
  elements.runTab.addEventListener("click", showRunView);
  elements.brandHome.addEventListener("click", showRunView);
  elements.visualizerTab.addEventListener("click", showVisualizerView);
  elements.visualizerTransition.addEventListener("change", () => {
    elements.visualizerRtStart.value = "0";
    elements.visualizerRtEnd.value = "0";
    elements.visualizerIntensity.value = "0";
    visualizer.rangeManuallySet = false;
    visualizer.referenceIds.clear();
    visualizer.uniformView = null;
    visualizer.pendingSave = null;
    updateReferenceButton();
    renderSelectedTransition({ preserveScroll: true });
  });
  elements.visualizerRefresh.addEventListener("click", () =>
    renderSelectedTransition({ preserveScroll: true }),
  );
  elements.visualizerCancel.addEventListener("click", cancelVisualizerRendering);
  elements.visualizerSave.addEventListener("click", () => saveBounds());
  elements.visualizerOverwriteBackup.addEventListener("click", () =>
    saveBounds({ overwrite: true }),
  );
  elements.visualizerApplyReferences.addEventListener("click", applyReferenceIntegration);
  elements.visualizerAutoShift.addEventListener("click", autoShiftFromReferences);
  elements.visualizerBackups.addEventListener("change", restoreSelectedBackup);
  elements.visualizerDeleteBackup.addEventListener("click", deleteSelectedBackup);
  elements.visualizerRenameBackup.addEventListener("click", renameSelectedBackup);
  elements.visualizerImportBackup.addEventListener("click", () => {
    showToast("Importing external EchoMSkit integration backups is not available yet.");
  });
  elements.visualizerSearch.addEventListener("input", () => {
    visualizer.search = elements.visualizerSearch.value ?? "";
    populateVisualizerTransitions();
  });
  elements.visualizerWidth.addEventListener("change", redrawVisualizerCharts);
  elements.visualizerHeight.addEventListener("change", redrawVisualizerCharts);
  elements.visualizerRtStart.addEventListener("change", () => {
    visualizer.rangeManuallySet =
      elements.visualizerRtStart.valueAsNumber > 0 ||
      elements.visualizerRtEnd.valueAsNumber > 0;
    visualizer.uniformView = null;
    redrawVisualizerCharts({ resetView: true });
  });
  elements.visualizerRtEnd.addEventListener("change", () => {
    visualizer.rangeManuallySet =
      elements.visualizerRtStart.valueAsNumber > 0 ||
      elements.visualizerRtEnd.valueAsNumber > 0;
    visualizer.uniformView = null;
    redrawVisualizerCharts({ resetView: true });
  });
  elements.visualizerIntensity.addEventListener("input", () => {
    if (!visualizer.rendering) {
      const yMax = elements.visualizerIntensity.valueAsNumber || 0;
      for (const chart of visualizer.charts) {
        if (yMax > 0) {
          chart.view.baseYMax = yMax;
          chart.view.yMax = yMax;
        } else {
          const domain = visualizerDomain(chart);
          chart.view.baseYMax =
            (maximumInRange(chart.sample.points, domain.rtStart, domain.rtEnd) ||
              chart.well.height ||
              visualizer.data?.globalIntensityMax ||
              1) * 1.1;
          chart.view.yMax = chart.view.baseYMax;
        }
        redrawChart(chart);
      }
    }
  });
  elements.visualizerSelectorExpand.addEventListener("click", () => {
    const collapsed = elements.visualizerToolbar.classList.toggle("selector-collapsed");
    const label = collapsed ? "expand graph selector" : "collapse graph selector";
    elements.visualizerSelectorExpand.setAttribute("aria-expanded", String(!collapsed));
    elements.visualizerSelectorExpand.setAttribute("aria-label", label);
    elements.visualizerSelectorExpand.title = label;
  });
  elements.themeToggle.addEventListener("click", toggleTheme);
  elements.guiScale.addEventListener("change", () =>
    applyGuiScale(elements.guiScale.value),
  );
  window.addEventListener("resize", () =>
    updateGuiScaleLayout(Number(elements.guiScale.value) || 100),
  );
  elements.clearActivity.addEventListener("click", () => {
    elements.activityLog.innerHTML = '<p class="empty-log">Activity cleared.</p>';
  });
  for (const link of elements.externalSlingLinks) {
    link.addEventListener("click", openExternalSling);
  }
  document.addEventListener("keydown", (event) => {
    if (elements.visualizerView.classList.contains("hidden")) return;
    const target = event.target;
    const shortcutToggleFocused = target === elements.visualizerShortcutGlobal;
    const typing =
      (target instanceof HTMLInputElement && !shortcutToggleFocused) ||
      target instanceof HTMLTextAreaElement ||
      target?.isContentEditable;
    const collapsed = elements.visualizerToolbar.classList.contains("selector-collapsed");
    const shortcutsEnabled =
      !collapsed || elements.visualizerShortcutGlobal.checked !== false;
    if (typing || visualizer.rendering || !shortcutsEnabled) return;
    if (event.key === "ArrowUp" || event.key === "ArrowDown") {
      event.preventDefault();
      const delta = event.key === "ArrowDown" ? 1 : -1;
      const next = Math.max(
        0,
        Math.min(
          elements.visualizerTransition.options.length - 1,
          elements.visualizerTransition.selectedIndex + delta,
        ),
      );
      if (next !== elements.visualizerTransition.selectedIndex) {
        elements.visualizerTransition.selectedIndex = next;
        previewSelectedTransition();
      }
    } else if (event.key === "Enter") {
      event.preventDefault();
      elements.visualizerRtStart.value = "0";
      elements.visualizerRtEnd.value = "0";
      elements.visualizerIntensity.value = "0";
      visualizer.rangeManuallySet = false;
      visualizer.referenceIds.clear();
      visualizer.uniformView = null;
      visualizer.pendingSave = null;
      updateReferenceButton();
      renderSelectedTransition({ preserveScroll: true });
    } else if (event.key === "ArrowLeft") {
      event.preventDefault();
      renderSelectedTransition({ preserveScroll: true });
    } else if (event.key === "=" || event.key === "+" || event.key === "-") {
      const chart = fallbackZoomChart();
      if (!chart) return;
      event.preventDefault();
      zoomChartHorizontal(
        chart,
        event.key === "-" ? 0.8 : 1.25,
        chart.hoverX,
      );
    }
  });

  await registerDesktopEvents();
  updateThemeToggle();

  if (!isDesktop) {
    renderProject(mockProject);
    addActivity("Browser preview mode is active.");
    return;
  }

  try {
    const state = await invoke("load_startup_state");
    if (state.theme === "light" || state.theme === "dark") {
      document.documentElement.dataset.theme = state.theme;
      updateThemeToggle();
    }
    if (state.project) {
      renderProject(state.project);
      addActivity(`Restored ${state.project.name}.`);
    } else {
      if (state.needsReselection) {
        elements.modalMessage.textContent =
          `The previous folder (${state.rememberedPath}) is no longer available. Choose its new location.`;
      }
      elements.projectModal.classList.remove("hidden");
    }
  } catch (error) {
    elements.projectModal.classList.remove("hidden");
    showToast(String(error), "error");
  }
}

bootstrap();
