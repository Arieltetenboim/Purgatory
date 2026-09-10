const typographyStep = document.createElement("link");
typographyStep.rel = "stylesheet";
typographyStep.href = "/typography_step.css";
document.head.appendChild(typographyStep);

const POOL_OPTIONS = [
  ["mandatory", "MANDATORY"],
  ["once", "ONCE"],
  ["repeatable", "REPEATABLE"],
  ["rare", "RARE"],
  ["lore", "LORE"],
];
const POOL_VALUES = new Set(POOL_OPTIONS.map(([value]) => value));
const poolEditor = document.getElementById("beatPoolEditor");
const poolMirror = document.getElementById("beatPool");

function syncPoolEditor() {
  if (!poolEditor) return;
  const beat = selectedBeat();
  poolEditor.disabled = !beat;
  poolEditor.value = beat?.pool || "once";
}

if (poolEditor) {
  for (const [value, label] of POOL_OPTIONS) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = label;
    poolEditor.appendChild(option);
  }

  poolEditor.addEventListener("change", () => {
    const beat = selectedBeat();
    if (!beat) return;
    beat.pool = poolEditor.value;
    renderBeatLists();
    changed();
  });

  if (poolMirror) {
    new MutationObserver(syncPoolEditor).observe(poolMirror, {
      childList: true,
      characterData: true,
      subtree: true,
    });
  }
  syncPoolEditor();
}

const baseValidationErrors = validationErrors;
validationErrors = function poolAwareValidationErrors(doc) {
  const errors = baseValidationErrors(doc);
  beatsOf(doc).forEach((beat, index) => {
    if (!beat || typeof beat !== "object") return;
    const label = beat.id || `Beat ${index + 1}`;
    if (typeof beat.pool !== "string" || !POOL_VALUES.has(beat.pool)) {
      errors.push(`${label} pool must be one of: ${[...POOL_VALUES].join(", ")}.`);
    }
  });
  return errors;
};

const n7StateScript = document.createElement("script");
n7StateScript.src = "/state_hardening.js";
n7StateScript.addEventListener("load", () => {
  const n6PreviewScript = document.createElement("script");
  n6PreviewScript.src = "/test_preview.js";
  n6PreviewScript.addEventListener("load", () => {
    const n8FlowScript = document.createElement("script");
    n8FlowScript.src = "/flow_view.js";
    n8FlowScript.addEventListener("load", () => {
      const n9aPresentationScript = document.createElement("script");
      n9aPresentationScript.src = "/presentation_cues.js";
      n9aPresentationScript.addEventListener("load", () => {
        const n9aTestCueScript = document.createElement("script");
        n9aTestCueScript.src = "/test_presentation_cues.js";
        document.body.appendChild(n9aTestCueScript);
      });
      document.body.appendChild(n9aPresentationScript);
    });
    document.body.appendChild(n8FlowScript);
  });
  document.body.appendChild(n6PreviewScript);
});
document.body.appendChild(n7StateScript);
