// Vanilla, no build step, no CDN. Two jobs: filter the table, and keep the bulk
// action bar honest about how many rows are selected.
(() => {
  const table = document.querySelector("[data-triage-table]");
  if (table) {
    const rows = () => Array.from(table.querySelectorAll("tbody tr[data-status]"));
    const pills = Array.from(document.querySelectorAll("[data-filter]"));
    const setFilter = (value) => {
      pills.forEach((pill) => pill.setAttribute("aria-pressed", String(pill.dataset.filter === value)));
      rows().forEach((row) => {
        row.hidden = value !== "all" && row.dataset.status !== value;
      });
    };
    pills.forEach((pill) => pill.addEventListener("click", () => setFilter(pill.dataset.filter)));
    setFilter("all");
  }

  const boxes = Array.from(document.querySelectorAll("input[data-app]"));
  const bulk = document.querySelector("[data-bulk]");
  if (boxes.length && bulk) {
    const label = bulk.querySelector("[data-bulk-count]");
    const status = document.querySelector("[data-bulk-status]");
    const update = () => {
      const selected = boxes.filter((box) => box.checked).length;
      bulk.dataset.open = String(selected >= 2);
      if (label) label.textContent = `${selected} selected`;
      // One polite region per page, updated only when the count changes.
      if (status && status.textContent !== `${selected} candidates selected`) {
        status.textContent = `${selected} candidates selected`;
      }
    };
    boxes.forEach((box) => box.addEventListener("change", update));
    update();
  }
})();
