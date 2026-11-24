<script>
(() => {
  const data = JSON.parse(document.getElementById("lab-data").textContent);
  const gateSelect = document.getElementById("gate-select");
  const answerSelect = document.getElementById("answer-select");
  const verdict = document.getElementById("gate-verdict");
  const gateText = document.getElementById("gate-text");
  const gateLabel = document.getElementById("gate-label");
  const factRows = document.getElementById("fact-rows");

  const allAnswers = data.gates.flatMap((gate) => gate.answers);
  data.gates.forEach((gate, index) => {
    const option = document.createElement("option");
    option.value = String(index);
    option.textContent = `${gate.code} — ${gate.label} (${gate.version})`;
    gateSelect.append(option);
  });

  const renderGate = () => {
    const gate = data.gates[Number(gateSelect.value)];
    gateLabel.textContent = `${data.requisition} · ${data.role} · ${data.employer}`;
    answerSelect.textContent = "";
    allAnswers.forEach((answer) => {
      const option = document.createElement("option");
      option.value = answer.code;
      option.textContent = `${answer.code} — ${answer.phrase}`;
      if (gate.answers.some((allowed) => allowed.code === answer.code)) {
        option.dataset.allowed = "true";
      }
      answerSelect.append(option);
    });
    const allowed = gate.answers.some((answer) => answer.code === answerSelect.value);
    verdict.textContent = allowed
      ? `Allowed: ${gate.code} offers this answer, so a GATE disposition may be recorded.`
      : `Blocked: ${gate.code} does not offer this answer. The form rejects it (DisallowedAnswer) before anything is written.`;
    verdict.className = `verdict ${allowed ? "ok" : "bad"}`;
    gateText.textContent = `${gate.requirement}\n\nWhat satisfies it: ${gate.satisfies}\n\n${gate.flexibility}`;
    factRows.textContent = "";
    gate.facts.forEach((fact) => {
      const row = document.createElement("tr");
      const key = document.createElement("td");
      key.className = "mono";
      key.textContent = fact.key;
      const value = document.createElement("td");
      value.textContent = fact.value || "MISSING";
      row.append(key, value);
      factRows.append(row);
    });
  };
  gateSelect.addEventListener("change", renderGate);
  answerSelect.addEventListener("change", renderGate);
  renderGate();

  const emailButtons = document.getElementById("email-buttons");
  const emailSubject = document.getElementById("email-subject");
  const emailBody = document.getElementById("email-body");
  const emailMeta = document.getElementById("email-meta");
  const renderEmail = (index) => {
    const email = data.emails[index];
    emailSubject.textContent = email.subject;
    emailBody.textContent = email.body;
    emailMeta.textContent = `Template ${email.letter} · ${email.kind} · rendered from the fixture`;
  };
  data.emails.forEach((email, index) => {
    const button = document.createElement("button");
    button.className = "pill";
    button.textContent = `Template ${email.letter}`;
    button.addEventListener("click", () => {
      emailButtons.querySelectorAll("button").forEach((other) => other.setAttribute("aria-pressed", "false"));
      button.setAttribute("aria-pressed", "true");
      renderEmail(index);
    });
    emailButtons.append(button);
  });
  if (data.emails.length) {
    emailButtons.querySelector("button")?.setAttribute("aria-pressed", "true");
    renderEmail(0);
  }

  const toHex = (buffer) =>
    Array.from(new Uint8Array(buffer)).map((byte) => byte.toString(16).padStart(2, "0")).join("");
  const digest = async (text) =>
    toHex(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text)));

  const rows = document.getElementById("chain-rows");
  const status = document.getElementById("chain-status");
  const entries = data.chain.map((entry) => ({ ...entry, current: entry.payload, expectMatch: true }));
  const renderChain = async () => {
    rows.textContent = "";
    for (const entry of entries) {
      const computed = await digest(entry.current);
      const matches = computed === entry.entry_hash;
      const row = document.createElement("tr");
      const add = (text, className) => {
        const cell = document.createElement("td");
        cell.className = className;
        cell.textContent = text;
        row.append(cell);
      };
      add(String(entry.seq), "mono");
      add(entry.send_id, "mono");
      add(entry.application_id, "mono");
      add(`${entry.entry_hash.slice(0, 16)}…`, "mono");
      add(matches ? "match" : "MISMATCH", `mono ${matches ? "ok" : "bad"}`);
      rows.append(row);
    }
    status.textContent = entries.every((entry) => entry.expectMatch)
      ? "All three digests match their recorded hashes."
      : "A digest does not match: the log would be refused.";
  };
  document.getElementById("verify").addEventListener("click", renderChain);
  document.getElementById("tamper").addEventListener("click", () => {
    if (entries[0]) {
      const original = entries[0].current;
      entries[0].current = original.includes("Ada")
        ? original.replace("Ada", "Eve")
        : original.replace('"body":"', '"body":"tampered ');
      if (entries[0].current === original) {
        entries[0].current = original.slice(0, 20) + "x" + original.slice(21);
      }
      entries[0].expectMatch = false;
    }
    renderChain();
  });
  renderChain();
})();
</script>