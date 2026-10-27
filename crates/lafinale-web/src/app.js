"use strict";

const route = ["/", "/importar", "/parchear"].includes(location.pathname) ? location.pathname : "/";
document.querySelectorAll("[data-view]").forEach((view) => { view.hidden = view.dataset.view !== route; });
document.querySelectorAll("[data-nav]").forEach((link) => {
  if (link.dataset.nav === route) link.setAttribute("aria-current", "page");
});
document.title = `${{ "/": "Inicio", "/importar": "Importar", "/parchear": "Parchear" }[route]} · LaFinale`;

const $ = (id) => document.getElementById(id);
const buildButton = $("build-exe");
const importButton = $("import-start");
let working = false;

function badge(id, found, yes = "Encontrado", no = "No encontrado") {
  const element = $(id);
  element.textContent = found ? yes : no;
  element.classList.toggle("found", found);
  element.classList.toggle("missing", !found);
}

async function refreshStatus() {
  try {
    const response = await fetch("/api/status", { cache: "no-store" });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const status = await response.json();
    const pak = status.inputs.script_pak_present;
    const exe = status.inputs.exe_present;
    const count = status.workspace.translation_json_count;
    const exeOutput = status.workspace.diagnostic_exe_present;
    badge("pak-input", pak);
    badge("pak-import", pak);
    badge("pak-patch-input", pak);
    badge("exe-input", exe);
    badge("exe-patch-input", exe);
    badge("exe-output", exeOutput);
    badge("exe-patch-output", exeOutput);
    badge("pak-output", status.workspace.output_pak_present);
    badge("json-count", count > 0, count === 1 ? "1 JSON" : `${count} JSON`, "0 JSON");
    badge("json-patch-count", count > 0, count === 1 ? "1 JSON" : `${count} JSON`, "0 JSON");
    if (count === null) {
      $("json-count").textContent = "No disponible";
      $("json-patch-count").textContent = "No disponible";
    }
    buildButton.disabled = working || !exe || exeOutput;
    importButton.disabled = working || !pak || count !== 0;
    $("status-error").hidden = true;
  } catch {
    buildButton.disabled = true;
    importButton.disabled = true;
    $("status-error").hidden = false;
  }
}

document.querySelectorAll("[data-refresh]").forEach((button) => button.addEventListener("click", refreshStatus));
buildButton.addEventListener("click", async () => {
  if (working || !window.confirm("Se creará output/SolfegeLF_DIAGNOSTIC.exe a partir del EXE original. No se instalará en el juego. ¿Continuar?")) return;
  working = true;
  buildButton.disabled = true;
  const message = $("build-message");
  message.hidden = false;
  message.classList.remove("error");
  message.textContent = "Verificando y generando el EXE de prueba…";
  try {
    const response = await fetch("/api/exe/build", { method: "POST", headers: { "Accept": "application/json" } });
    const result = await response.json();
    if (!response.ok) throw new Error(result.error || `Error HTTP ${response.status}`);
    message.textContent = `Creado ${result.output}. Es solo una prueba: no lo instales como parche final.`;
  } catch (error) {
    message.classList.add("error");
    message.textContent = error.message || "No se pudo generar el EXE.";
  } finally {
    working = false;
    await refreshStatus();
  }
});

importButton.addEventListener("click", async () => {
  if (working || !window.confirm("Se crearán JSON en work/translations/ desde tu Script.pak original. La carpeta debe estar vacía. ¿Continuar?")) return;
  working = true;
  importButton.disabled = true;
  const message = $("import-message");
  message.hidden = false;
  message.classList.remove("error");
  message.textContent = "Validando PAK y extrayendo los diálogos…";
  try {
    const response = await fetch("/api/import", { method: "POST", headers: { "Accept": "application/json" } });
    const result = await response.json();
    if (!response.ok) throw new Error(result.error || `Error HTTP ${response.status}`);
    message.textContent = `Listos ${result.scripts} JSON y ${result.strings.toLocaleString("es-ES")} cadenas en ${result.destination}. Sin traducciones aplicadas todavía.`;
  } catch (error) {
    message.classList.add("error");
    message.textContent = error.message || "No se pudo importar el PAK.";
  } finally {
    working = false;
    await refreshStatus();
  }
});

refreshStatus();
