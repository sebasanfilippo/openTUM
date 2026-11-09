// Utilidades compartidas por las ventanas web.
const T = window.__TAURI__;
const invoke = (cmd, args) => T.core.invoke(cmd, args);
const escuchar = (ev, fn) => T.event.listen(ev, (e) => fn(e.payload));

const $ = (sel) => document.querySelector(sel);
const esc = (s) => String(s ?? "").replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));

const ETIQUETAS = { sesion_5h: "Ventana de 5 horas", semanal: "Semanal", semanal_sonnet: "Semanal (Sonnet)",
  creditos_adicionales: "Créditos adicionales", saldo_creditos: "Saldo de créditos", saldo_prepagado: "Saldo prepagado",
  gasto_periodo: "Gasto del período", gemini_5h: "Gemini · 5 horas", gemini_semanal: "Gemini · semanal",
  claude_gpt_5h: "Claude y GPT · 5 horas", claude_gpt_semanal: "Claude y GPT · semanal",
  gemini_4_argon_semanal: "Gemini 4 Argon · semanal" };
const nombreMetrica = (n) => ETIQUETAS[n] || n.replace(/_/g, " ");
const UNIDADES = { PORCENTAJE: "porcentaje", TOKENS: "tokens", SOLICITUDES: "solicitudes", CREDITOS: "créditos", USD: "dólares" };
const ORIGENES = { CLI: "CLI", ARCHIVO_LOCAL: "archivo local", API: "API", CABECERA: "cabecera de respuesta" };
const LIMITES = { CUOTA: "cuota", LIMITE_TASA: "límite de tasa", TOPE_GASTO: "tope de gasto", SALDO: "saldo" };
const INICIALES = { claude_code: "CC", codex: "CX", xai: "X", antigravity: "AG" };

function fmtValor(m) {
  if (m.unidad === "PORCENTAJE") return `${m.valor.toFixed(0)} %${m.sentido === "DISPONIBLE" ? " disponible" : ""}`;
  if (m.unidad === "USD") return `USD ${m.valor.toFixed(2)}`;
  return `${m.valor.toLocaleString("es-AR")} ${UNIDADES[m.unidad]}`;
}
function fmtFecha(iso) {
  if (!iso) return "—";
  const d = new Date(iso), hoy = new Date();
  const h = d.toLocaleTimeString("es-AR", { hour: "2-digit", minute: "2-digit" });
  return d.toDateString() === hoy.toDateString() ? h : `${d.toLocaleDateString("es-AR", { day: "2-digit", month: "2-digit" })} ${h}`;
}
function hasta(iso) {
  if (!iso) return "";
  const min = Math.round((new Date(iso) - new Date()) / 60000);
  if (min <= 0) return "reinicio inminente";
  const d = Math.floor(min / 1440), h = Math.floor((min % 1440) / 60), m = min % 60;
  return "reinicio en " + (d ? `${d} d ${h} h` : h ? `${h} h ${m} min` : `${m} min`);
}
function color(p, umbrales) {
  if (p == null) return "var(--gris)";
  if (p >= umbrales[umbrales.length - 1]) return "var(--rojo)";
  if (p >= umbrales[0]) return "var(--ambar)";
  return "var(--verde)";
}
function claseEstado(e) {
  if (e === "ACTUALIZADO") return "";
  if (e === "DESACTUALIZADO" || e === "LIMITE_CONSULTA") return "aviso";
  return "error";
}
