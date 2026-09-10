(() => {
  const speech = document.getElementById("testSpeech");
  const meta = document.getElementById("testBeatMeta");
  if (!speech || !meta) return;

  function renderCues() {
    const beatId = String(meta.textContent || "").split(" · ")[0].trim();
    const beat = beatsOf().find((candidate) => candidate?.id === beatId) || null;
    const renderedLines = [...speech.querySelectorAll(".player-line")];
    if (!beat || !renderedLines.length) return;

    const authoredLines = (Array.isArray(beat.lines) ? beat.lines : []).filter(
      (line) => line && typeof line.text === "string" && line.text.length > 0
    );
    renderedLines.forEach((element, index) => {
      element.querySelector(".test-animation-cue")?.remove();
      const animation = authoredLines[index]?.animation;
      if (typeof animation !== "string" || !animation.trim()) return;
      const cue = document.createElement("div");
      cue.className = "test-animation-cue";
      cue.textContent = `Animation cue: ${animation.trim()}`;
      element.appendChild(cue);
    });
  }

  new MutationObserver(renderCues).observe(speech, {childList: true, subtree: true});
  new MutationObserver(renderCues).observe(meta, {childList: true, characterData: true, subtree: true});
  renderCues();
})();
