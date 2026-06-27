import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open, save } from "@tauri-apps/plugin-dialog";
import { load, Store } from "@tauri-apps/plugin-store";

interface CopiedFile {
  fileName: string;
  relPath: string;
  size: number;
  action: string;
}
interface NameResult {
  name: string;
  count: number;
}
interface FilterResult {
  requested: number;
  matchedNames: number;
  copiedCount: number;
  skippedCount: number;
  totalBytes: number;
  names: NameResult[];
  copied: CopiedFile[];
  notFound: string[];
  errors: string[];
  dryRun: boolean;
}

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

const els = {
  pickSource: $("pick-source") as HTMLButtonElement,
  pickDest: $("pick-dest") as HTMLButtonElement,
  sourcePath: $("source-path"),
  destPath: $("dest-path"),
  names: $("names") as HTMLTextAreaElement,
  recursive: $("recursive") as HTMLInputElement,
  conflict: $("conflict") as HTMLSelectElement,
  extMode: $("ext-mode") as HTMLSelectElement,
  customExt: $("custom-ext") as HTMLInputElement,
  scan: $("scan") as HTMLButtonElement,
  copy: $("copy") as HTMLButtonElement,
  statRequested: $("stat-requested"),
  statMatched: $("stat-matched"),
  statCopied: $("stat-copied"),
  statCopiedLabel: $("stat-copied-label"),
  progress: $("progress"),
  progressBar: $("progress-bar"),
  progressText: $("progress-text"),
  placeholder: $("placeholder"),
  resultList: $("result-list") as HTMLUListElement,
  export: $("export") as HTMLButtonElement,
  openDest: $("open-dest") as HTMLButtonElement,
  toast: $("toast"),
};

let sourceDir = "";
let destDir = "";
let lastResult: FilterResult | null = null;
let store: Store | null = null;

// ---------- helpers ----------
function toast(msg: string, err = false) {
  els.toast.textContent = msg;
  els.toast.classList.toggle("err", err);
  els.toast.classList.remove("hidden");
  window.setTimeout(() => els.toast.classList.add("hidden"), 3200);
}

function humanSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let v = bytes / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(1)} ${units[i]}`;
}

function parsedNames(): string[] {
  return els.names.value
    .split(/\r?\n/)
    .map((s) => s.trim())
    .filter(Boolean);
}

function updateNameCount() {
  const n = new Set(
    parsedNames().map((l) => l.replace(/\.[^./\\]+$/, "").toLowerCase())
  ).size;
  els.statRequested.textContent = String(n);
}

function setFolder(kind: "source" | "dest", path: string) {
  // hiển thị tên thư mục cho dễ đọc, đường dẫn đầy đủ làm tooltip
  const name = path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || path;
  if (kind === "source") {
    sourceDir = path;
    els.sourcePath.textContent = name;
    els.sourcePath.title = path;
    els.pickSource.classList.add("filled");
    store?.set("source", path);
  } else {
    destDir = path;
    els.destPath.textContent = name;
    els.destPath.title = path;
    els.pickDest.classList.add("filled");
    store?.set("dest", path);
  }
  store?.save();
}

// ---------- folder pickers ----------
async function pick(kind: "source" | "dest") {
  const selected = await open({
    directory: true,
    multiple: false,
    defaultPath: kind === "source" ? sourceDir : destDir || sourceDir,
  });
  if (typeof selected === "string") setFolder(kind, selected);
}

els.pickSource.addEventListener("click", () => pick("source"));
els.pickDest.addEventListener("click", () => pick("dest"));

// ---------- ext mode ----------
els.extMode.addEventListener("change", () => {
  els.customExt.classList.toggle("hidden", els.extMode.value !== "custom");
});

els.names.addEventListener("input", updateNameCount);

// ---------- run ----------
async function run(dryRun: boolean) {
  if (!sourceDir) {
    toast("Chưa chọn thư mục nguồn.", true);
    return;
  }
  if (!dryRun && !destDir) {
    toast("Chưa chọn thư mục đích.", true);
    return;
  }
  const names = parsedNames();
  if (names.length === 0) {
    toast("Danh sách tên ảnh đang trống.", true);
    return;
  }

  els.scan.disabled = true;
  els.copy.disabled = true;
  els.progress.classList.remove("hidden");
  els.progressBar.style.width = "0%";
  els.progressText.textContent = dryRun ? "Đang quét…" : "Đang chép…";

  try {
    const res = await invoke<FilterResult>("filter_photos", {
      req: {
        source: sourceDir,
        dest: destDir,
        names,
        extMode: els.extMode.value,
        customExts: els.customExt.value
          .split(/[,\s]+/)
          .map((s) => s.trim())
          .filter(Boolean),
        recursive: els.recursive.checked,
        onConflict: els.conflict.value,
        dryRun,
      },
    });
    lastResult = res;
    render(res);
  } catch (e) {
    toast(String(e), true);
  } finally {
    els.scan.disabled = false;
    els.copy.disabled = false;
    els.progress.classList.add("hidden");
  }
}

els.scan.addEventListener("click", () => run(true));
els.copy.addEventListener("click", () => run(false));

// ---------- progress events ----------
listen<{ phase: string; done: number; total: number }>(
  "filter-progress",
  (e) => {
    const { phase, done, total } = e.payload;
    if (total === 0) return;
    const pct = Math.round((done / total) * 100);
    els.progressBar.style.width = `${pct}%`;
    els.progressText.textContent =
      phase === "scan"
        ? `Tìm thấy ${total} ảnh khớp`
        : `Đang chép ${done}/${total}`;
  }
);

// ---------- render ----------
function render(res: FilterResult) {
  els.statRequested.textContent = String(res.requested);
  els.statMatched.textContent = String(res.matchedNames);
  els.statCopied.textContent = String(res.copiedCount);
  els.statCopiedLabel.textContent = res.dryRun ? "ảnh khớp" : "đã chép";
  els.placeholder.classList.add("hidden");
  els.export.classList.remove("hidden");
  els.resultList.innerHTML = "";

  for (const n of res.names) {
    const li = document.createElement("li");
    const name = document.createElement("span");
    name.className = "name";
    name.textContent = n.name;
    const badge = document.createElement("span");
    if (n.count > 0) {
      badge.className = "badge ok";
      badge.textContent = `${n.count} ảnh`;
    } else {
      badge.className = "badge miss";
      badge.textContent = "không thấy";
    }
    li.append(name, badge);
    els.resultList.append(li);
  }

  if (res.dryRun) {
    toast(
      `Khớp ${res.matchedNames}/${res.requested} tên • ${res.copiedCount} ảnh.`
    );
  } else {
    els.openDest.classList.remove("hidden");
    const skip = res.skippedCount ? `, bỏ qua ${res.skippedCount}` : "";
    const err = res.errors.length ? `, ${res.errors.length} lỗi` : "";
    toast(
      `Đã chép ${res.copiedCount} ảnh (${humanSize(res.totalBytes)})${skip}${err}.`,
      res.errors.length > 0
    );
  }
}

// ---------- open destination ----------
els.openDest.addEventListener("click", async () => {
  if (!destDir) return;
  try {
    await invoke("open_dir", { path: destDir });
  } catch (e) {
    toast(String(e), true);
  }
});

// ---------- export report ----------
els.export.addEventListener("click", async () => {
  if (!lastResult) return;
  const r = lastResult;
  const lines: string[] = [];
  lines.push("=== BÁO CÁO LỌC ẢNH — MPhotoPicker ===");
  lines.push(`Nguồn:  ${sourceDir}`);
  lines.push(`Đích:   ${destDir || "(chỉ quét)"}`);
  lines.push(
    `Tên nhập: ${r.requested} | Khớp: ${r.matchedNames} | Ảnh: ${r.copiedCount} | Bỏ qua: ${r.skippedCount}`
  );
  lines.push("");
  lines.push(`--- KHÔNG TÌM THẤY (${r.notFound.length}) ---`);
  lines.push(...(r.notFound.length ? r.notFound : ["(không có)"]));
  lines.push("");
  lines.push(`--- FILE ${r.dryRun ? "KHỚP" : "ĐÃ CHÉP"} (${r.copied.length}) ---`);
  for (const c of r.copied) {
    lines.push(`[${c.action}] ${c.relPath} (${humanSize(c.size)})`);
  }
  if (r.errors.length) {
    lines.push("");
    lines.push(`--- LỖI (${r.errors.length}) ---`);
    lines.push(...r.errors);
  }

  const path = await save({
    defaultPath: "bao-cao-loc-anh.txt",
    filters: [{ name: "Text", extensions: ["txt"] }],
  });
  if (path) {
    await invoke("write_report", { path, content: lines.join("\n") });
    toast("Đã lưu báo cáo.");
  }
});

// ---------- drag & drop folders ----------
function cardUnderPoint(x: number, y: number): "source" | "dest" | null {
  const el = document.elementFromPoint(x, y);
  if (!el) return null;
  if (el.closest("#pick-source")) return "source";
  if (el.closest("#pick-dest")) return "dest";
  return null;
}

getCurrentWebview().onDragDropEvent((event) => {
  const dpr = window.devicePixelRatio || 1;
  if (event.payload.type === "over") {
    const { x, y } = event.payload.position;
    const target = cardUnderPoint(x / dpr, y / dpr);
    els.pickSource.classList.toggle("dragover", target === "source");
    els.pickDest.classList.toggle("dragover", target === "dest");
  } else if (event.payload.type === "drop") {
    els.pickSource.classList.remove("dragover");
    els.pickDest.classList.remove("dragover");
    const { x, y } = event.payload.position;
    const target = cardUnderPoint(x / dpr, y / dpr);
    const path = event.payload.paths[0];
    if (target && path) {
      setFolder(target, path);
      toast(`Đã đặt thư mục ${target === "source" ? "nguồn" : "đích"}.`);
    }
  } else {
    els.pickSource.classList.remove("dragover");
    els.pickDest.classList.remove("dragover");
  }
});

// ---------- restore last folders ----------
(async () => {
  try {
    store = await load("settings.json", { autoSave: false });
    const s = await store.get<string>("source");
    const d = await store.get<string>("dest");
    if (s) setFolder("source", s);
    if (d) setFolder("dest", d);
  } catch {
    /* store optional */
  }
})();
