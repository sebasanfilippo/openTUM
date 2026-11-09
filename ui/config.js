// Configuración (HU-01, HU-02, HU-15, HU-17, HU-18).
let cfg = null;
const mostrar = (el, texto, error) => { el.hidden = false; el.textContent = texto; el.className = "mensaje" + (error ? " error" : ""); };
function cargar() {
  invoke("config_obtener").then((c) => {
    cfg = c;
    document.querySelectorAll("[data-prov]").forEach((cb) => {
      const p = c.proveedores.find((x) => x.id === cb.dataset.prov);
      cb.checked = !!(p && p.habilitado);
      if (cb.dataset.prov === "xai") cb.disabled = !p;
    });
    const xai = c.proveedores.find((x) => x.id === "xai");
    $("#xaiEstado").textContent = xai ? `Team ID ${xai.teamId} · clave en el llavero` : "Requiere Team ID y clave de gestión.";
    if (xai) $("#teamId").value = xai.teamId;
    $("#u1").value = c.umbrales[0]; $("#u2").value = c.umbrales[c.umbrales.length - 1];
    $("#saldo").value = c.umbralSaldoUSD ?? "";
    $("#silencio").checked = !!c.horarioSilencio;
    $("#desde").value = c.horarioSilencio ? c.horarioSilencio.desde : "22:00";
    $("#hasta").value = c.horarioSilencio ? c.horarioSilencio.hasta : "07:00";
    $("#intervalo").value = Math.round(c.intervaloSeg / 60);
  });
}
$("#btnGuardar").onclick = () => {
  const min = Number($("#intervalo").value);
  if (!Number.isInteger(min) || min < 2 || min > 60) return mostrar($("#msg"), "El intervalo debe estar entre 2 y 60 minutos.", true);
  const c = structuredClone(cfg);
  document.querySelectorAll("[data-prov]").forEach((cb) => {
    let p = c.proveedores.find((x) => x.id === cb.dataset.prov);
    if (!p && cb.dataset.prov !== "xai") { p = { id: cb.dataset.prov, habilitado: false, mecanismo: cb.dataset.prov === "claude_code" ? "ARCHIVO_LOCAL" : "CLI" }; c.proveedores.push(p); }
    if (p) p.habilitado = cb.checked;
  });
  const u1 = Number($("#u1").value), u2 = Number($("#u2").value);
  c.umbrales = u1 === u2 ? [u1] : [u1, u2];
  c.umbralSaldoUSD = $("#saldo").value === "" ? undefined : Number($("#saldo").value);
  c.horarioSilencio = $("#silencio").checked ? { desde: $("#desde").value, hasta: $("#hasta").value } : undefined;
  c.intervaloSeg = min * 60;
  invoke("config_guardar", { config: c }).then(() => { mostrar($("#msg"), "Configuración guardada."); cargar(); })
    .catch((e) => mostrar($("#msg"), "No se guardó: " + e, true));
};
$("#btnClave").onclick = () => {
  $("#btnClave").disabled = true;
  invoke("xai_registrar", { teamId: $("#teamId").value, clave: $("#clave").value })
    .then((m) => { $("#clave").value = ""; mostrar($("#msgClave"), m); cargar(); })
    .catch((e) => mostrar($("#msgClave"), String(e), true))
    .finally(() => { $("#btnClave").disabled = false; });
};
$("#btnEliminar").onclick = () => {
  if (!confirm("Se eliminarán las claves guardadas por openTUM y sus archivos (config.json, lecturas.json y alertas.json). Las sesiones de las CLI no se modifican. ¿Continuar?")) return;
  invoke("eliminar_datos").then((m) => { mostrar($("#msg"), m); cargar(); }).catch((e) => mostrar($("#msg"), String(e), true));
};
$("#btnPanel").onclick = () => invoke("abrir_ventana", { destino: "panel" });
cargar();
