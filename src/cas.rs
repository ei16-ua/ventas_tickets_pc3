use std::sync::atomic::Ordering;

use crate::venue::SectorNumerado;

/// Intenta cambiar todas las butacas de un estado a otro con CAS.
///
/// Si una butaca falla, revierte las anteriores y devuelve su índice.
/// El rollback usa CAS (no store) para no sobreescribir un cambio
/// que otro hilo haya hecho entre medias.
#[allow(dead_code)]
#[inline(always)]
pub(crate) fn cambiar_todas(
    sector: &SectorNumerado,
    idxs: &[usize],
    desde: u8,
    hasta: u8,
) -> Result<(), usize> {
    for (k, &i) in idxs.iter().enumerate() {
        if sector.seats[i]
            .compare_exchange(desde, hasta, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            // Rollback seguro: solo revertimos si el asiento sigue en el
            // estado que NOSOTROS le pusimos (`hasta`). Si otro hilo ya lo
            // cambió (p.ej. a VENDIDA), no lo tocamos.
            for &j in &idxs[..k] {
                let _ = sector.seats[j].compare_exchange(
                    hasta, desde,
                    Ordering::AcqRel, Ordering::Acquire,
                );
            }
            return Err(i);
        }
    }
    Ok(())
}
