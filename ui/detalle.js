// Detalle de un proveedor (HU-04, HU-10).
const id = new URLSearchParams(location.search).get("id");
function pintar(s) {
  const p = s.proveedores.find((x) => x.id === id);
  if (!p) { $("#titulo").textContent = "Proveedor no habilitado"; $("#filas").innerHTML = ""; return; }
  $("#titulo").textContent = p.nombre;
  $("#estado").textContent = p.estadoEtiqueta;
  $("#estado").className = "etiqueta " + claseEstado(p.estado);
  const desact = p.estado === "DESACTUALIZADO" ? " El dato tiene más de dos intervalos de actualización de antigüedad." : "";
  $("#resumen").innerHTML = `Dato obtenido: <strong>${fmtFecha(p.obtenidaEn)}</strong>${desact}${p.detalle ? "<br>" + esc(p.detalle) : ""}`;
  $("#filas").innerHTML = p.metricas.map((m) => `<tr>
    <td>${esc(nombreMetrica(m.nombre))}<div class="muted">${esc(m.nombre)}</div></td>
    <td><strong>${esc(fmtValor(m))}</strong>${m.esEstimado ? '<div class="muted">estimado</div>' : ""}</td>
    <td>${UNIDADES[m.unidad]}<div class="muted">${m.sentido === "CONSUMIDO" ? "consumido" : "disponible"}</div></td>
    <td>${LIMITES[m.tipoLimite]}${m.limite != null ? `<div class="muted">${esc(m.limite)}</div>` : ""}</td>
    <td>${m.ventana && m.ventana.reinicioEn ? esc(fmtFecha(m.ventana.reinicioEn)) + `<div class="muted">${esc(m.ventana.duracion)}</div>` : (m.ventana ? esc(m.ventana.duracion) : "—")}</td>
    <td>${ORIGENES[m.origen]}</td></tr>`).join("") || `<tr><td colspan="6" class="muted">Sin datos.</td></tr>`;
}
$("#btnPanel").onclick = () => invoke("abrir_ventana", { destino: "panel" });
escuchar("estado", pintar);
invoke("estado").then(pintar);
