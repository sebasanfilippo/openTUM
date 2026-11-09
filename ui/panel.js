// Panel consolidado (HU-09, HU-11, HU-12, HU-14).
function tarjeta(p, umbrales) {
  const filas = p.metricas.map((m) => {
    const barra = m.porcentaje != null
      ? `<div class="barra"><span style="width:${m.porcentaje.toFixed(1)}%;background:${color(m.porcentaje, umbrales)}"></span></div>` : "";
    const reinicio = m.ventana && m.ventana.reinicioEn ? hasta(m.ventana.reinicioEn) : "";
    const estimado = m.esEstimado ? " · estimado" : "";
    return `<div class="fila"><div class="cab"><span>${esc(nombreMetrica(m.nombre))}</span><strong>${esc(fmtValor(m))}${m.limite != null && m.unidad !== "PORCENTAJE" ? " / " + esc(m.limite.toFixed(2)) : ""}</strong></div>
      ${barra}<div class="muted">${esc(reinicio)}${estimado}</div></div>`;
  }).join("");
  const sinDatos = p.metricas.length ? "" : `<p class="muted">Sin datos: ${esc(p.estadoEtiqueta)}.</p>`;
  const det = p.estado !== "ACTUALIZADO" && p.detalle ? `<p class="mensaje ${claseEstado(p.estado)}">${esc(p.detalle)}</p>` : "";
  return `<section class="tarjeta"><h2><span class="inicial">${INICIALES[p.id]}</span>${esc(p.nombre)}
      <span class="etiqueta ${claseEstado(p.estado)}">${esc(p.estadoEtiqueta)}</span></h2>
    ${filas}${sinDatos}${det}
    <div class="muted">Dato obtenido: ${fmtFecha(p.obtenidaEn)}</div>
    <div class="acciones"><button data-detalle="${p.id}">Ver detalle</button></div></section>`;
}
function pintar(s) {
  $("#tarjetas").innerHTML = s.proveedores.length
    ? s.proveedores.map((p) => tarjeta(p, s.umbrales)).join("")
    : `<section class="tarjeta"><p>No hay proveedores habilitados.</p><button class="primario" id="irConfig">Abrir configuración</button></section>`;
  $("#actualizado").textContent = s.actualizando ? "Actualizando…" : s.ultimaActualizacion ? `Actualizado ${fmtFecha(s.ultimaActualizacion)}` : "";
  $("#btnActualizar").disabled = s.actualizando;
  document.querySelectorAll("[data-detalle]").forEach((b) => b.onclick = () => invoke("abrir_ventana", { destino: "detalle:" + b.dataset.detalle }));
  const ir = $("#irConfig"); if (ir) ir.onclick = () => invoke("abrir_ventana", { destino: "config" });
}
$("#btnActualizar").onclick = () => invoke("actualizar");
$("#btnConfig").onclick = () => invoke("abrir_ventana", { destino: "config" });
escuchar("estado", pintar);
invoke("estado").then(pintar);
setInterval(() => invoke("estado").then(pintar), 30000);
