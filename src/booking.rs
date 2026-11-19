use std::sync::atomic::Ordering;
use std::sync::Arc;

use rand::Rng;

#[path = "cas.rs"]
mod cas;
use crate::config;
use crate::venue;
use crate::venue::{ConcurrentVenue, SectorConcurrente, SectorGeneral, SectorNumerado};
use crate::venue::{LIBRE, SELECCIONADA, VENDIDA};

/// Trait que define las operaciones del motor de reservas (*Booker*).
///
/// Modela la interfaz que utiliza la simulación (`simulation.rs`) para consultar
/// disponibilidad, reservar provisionalmente localidades y consolidar la compra.
pub trait Booker {
    /// Nombre del recinto actual.
    #[allow(dead_code)]
    fn get_venue_name(&self) -> &str;

    /// Capacidad total acumulada de todos los sectores del recinto.
    fn get_total_capacity(&self) -> usize;

    /// Número total de localidades disponibles actualmente en el recinto.
    #[allow(dead_code)]
    fn get_total_available_seats(&self) -> usize;

    /// Lista de códigos de los sectores que aún disponen de plazas libres.
    fn get_available_sectors(&self) -> Vec<String>;

    /// Indica si un sector tiene asientos numerados (`true`) o es de admisión general (`false`).
    #[allow(dead_code)]
    fn is_sector_numbered(&self, sector_code: &str) -> bool;

    /// Lista de identificadores de localidades disponibles en el sector indicado.
    fn get_available_seats(&self, sector_code: &str) -> Vec<String>;

    /// Número de entradas que faltan por vender en todo el recinto.
    fn remaining_seats(&self) -> usize;

    /// Notifica que una sesión de compra va a interactuar con un sector.
    fn select_sector(&mut self, sector_code: &str);

    /// Notifica que una sesión de compra abandona el sector sin completar la compra.
    fn unselect_sector(&mut self, sector_code: &str);

    /// Cancela la reserva provisional de las localidades indicadas,
    /// devolviéndolas al estado `Available`.
    fn deselect_seats(&mut self, sector_code: &str, seat_ids: &[String]) -> Result<(), String>;

    /// Bloquea temporalmente las localidades indicadas cambiando su estado a `Selected`.
    ///
    /// Retorna `Ok(())` si todas se han podido reservar provisionalmente o `Err` si alguna ya no estaba disponible.
    fn select_seats(&mut self, sector_code: &str, seat_ids: &[String]) -> Result<(), String>;

    /// Confirma definitivamente la compra de las localidades indicadas pasando su estado a `Booked`.
    ///
    /// Retorna `Ok(Vec<String>)` con los identificadores únicos definitivos de las entradas vendidas
    /// o `Err` si la reserva provisional no era válida o expiró.
    fn book_seats(&mut self, sector_code: &str, seat_ids: &[String]) -> Result<Vec<String>, String>;

    /// Devuelve una copia/instantánea del recinto (`Venue`) con el estado de todos sus sectores.
    fn get_venue(&self) -> venue::Venue;
}

/// Implementación secuencial de referencia del trait `Booker`.
///
/// Modifica directamente la estructura `Venue` en memoria sin sincronización multihilo.
#[allow(dead_code)]
pub struct SequentialBooker {
    venue: venue::Venue,
}

#[allow(dead_code)]
impl SequentialBooker {
    pub fn new(venue: venue::Venue) -> Self {
        Self { venue }
    }
}

#[allow(dead_code)]
impl Booker for SequentialBooker {
    fn get_venue_name(&self) -> &str {
        &self.venue.name
    }

    fn get_total_capacity(&self) -> usize {
        self.venue.total_capacity()
    }

    fn get_total_available_seats(&self) -> usize {
        self.venue.total_available_seats()
    }

    fn get_available_sectors(&self) -> Vec<String> {
        self.venue
            .sectors
            .iter()
            .filter(|s| s.n_available_seats() > 0)
            .map(|s| s.code.clone())
            .collect()
    }

    fn is_sector_numbered(&self, sector_code: &str) -> bool {
        self.venue
            .sectors
            .iter()
            .find(|s| s.code == sector_code)
            .map(|s| s.is_numbered)
            .unwrap_or(false)
    }

    fn get_available_seats(&self, sector_code: &str) -> Vec<String> {
        if let Some(sector) = self.venue.sectors.iter().find(|s| s.code == sector_code) {
            sector.get_available_seats().iter().map(|s| s.id.clone()).collect()
        } else {
            panic!("Unknown sector: '{sector_code}'");
        }
    }

    fn remaining_seats(&self) -> usize {
        self.venue.total_capacity() - self.venue.total_booked_seats()
    }

    fn select_sector(&mut self, _sector_code: &str) {
        // En la versión secuencial básica no requiere acción adicional.
    }

    fn unselect_sector(&mut self, _sector_code: &str) {
        // En la versión secuencial básica no requiere acción adicional.
    }

    fn deselect_seats(&mut self, sector_code: &str, seat_ids: &[String]) -> Result<(), String> {
        let sector = self
            .venue
            .sectors
            .iter_mut()
            .find(|s| s.code == sector_code)
            .ok_or_else(|| format!("Unknown sector: '{sector_code}'"))?;

        // 1. Validación: verificar que todos los asientos existen y están en estado Selected
        for seat_id in seat_ids {
            match sector.seats.iter().find(|s| s.id == *seat_id) {
                Some(seat) if seat.state == venue::SeatState::Selected => {}
                Some(_) => return Err(format!("Seat {} is not selected", seat_id)),
                None => return Err(format!("Seat {} does not exist in sector {}", seat_id, sector_code)),
            }
        }

        // 2. Transición de estado: Selected -> Available
        for seat_id in seat_ids {
            if let Some(seat) = sector.seats.iter_mut().find(|s| s.id == *seat_id) {
                seat.state = venue::SeatState::Available;
            }
        }

        Ok(())
    }

    fn select_seats(&mut self, sector_code: &str, seat_ids: &[String]) -> Result<(), String> {
        let sector = self
            .venue
            .sectors
            .iter_mut()
            .find(|s| s.code == sector_code)
            .ok_or_else(|| format!("Unknown sector: '{sector_code}'"))?;

        // 1. Validación: verificar que todos los asientos existen y están en estado Available
        for seat_id in seat_ids {
            match sector.seats.iter().find(|s| s.id == *seat_id) {
                Some(seat) if seat.state == venue::SeatState::Available => {}
                Some(_) => return Err(format!("Seat {} is not available", seat_id)),
                None => return Err(format!("Seat {} does not exist in sector {}", seat_id, sector_code)),
            }
        }

        // 2. Transición de estado: Available -> Selected
        for seat_id in seat_ids {
            if let Some(seat) = sector.seats.iter_mut().find(|s| s.id == *seat_id) {
                seat.state = venue::SeatState::Selected;
            }
        }

        Ok(())
    }

    fn book_seats(&mut self, sector_code: &str, seat_ids: &[String]) -> Result<Vec<String>, String> {
        let sector = self
            .venue
            .sectors
            .iter_mut()
            .find(|s| s.code == sector_code)
            .ok_or_else(|| format!("Unknown sector: '{sector_code}'"))?;

        // 1. Validación: verificar que todos los asientos existen y están en estado Selected
        for seat_id in seat_ids {
            match sector.seats.iter().find(|s| s.id == *seat_id) {
                Some(seat) if seat.state == venue::SeatState::Selected => {}
                Some(seat) if seat.state == venue::SeatState::Booked => {
                    return Err(format!("Seat {} is already booked", seat_id));
                }
                Some(_) => {
                    return Err(format!("Seat {} must be selected before booking", seat_id));
                }
                None => return Err(format!("Seat {} does not exist in sector {}", seat_id, sector_code)),
            }
        }

        // 2. Transición de estado: Selected -> Booked
        for seat_id in seat_ids {
            if let Some(seat) = sector.seats.iter_mut().find(|s| s.id == *seat_id) {
                seat.state = venue::SeatState::Booked;
            }
        }

        Ok(seat_ids.to_vec())
    }

    fn get_venue(&self) -> venue::Venue {
        self.venue.clone()
    }
}

// ===========================================================================
//  BOOKER CONCURRENTE (sin cerrojos)
//
//  - Sectores de admisión general: un contador atómico de plazas libres.
//  - Sectores numerados: un atómico por butaca, modificado con CAS.
//
//  El struct solo guarda un `Arc` al recinto compartido, así que clonarlo es
//  barato: cada hilo trabaja con SU clon (por eso el `&mut self` del trait no
//  es un problema) y todos los clones ven el mismo estado.
// ===========================================================================
#[allow(dead_code)]
#[derive(Clone)]
pub struct ConcurrentBooker {
    shared: Arc<ConcurrentVenue>,
}

#[allow(dead_code)]
impl ConcurrentBooker {
    pub fn new(venue: venue::Venue) -> Self {
        ConcurrentBooker {
            shared: Arc::new(ConcurrentVenue::from_venue(&venue)),
        }
    }

    fn sector(&self, sector_code: &str) -> Result<&SectorConcurrente, String> {
        self.shared
            .sector(sector_code)
            .ok_or_else(|| format!("Unknown sector: '{sector_code}'"))
    }

    /// Traduce los IDs de butaca a posiciones del vector de atómicos.
    fn indices(sector: &SectorNumerado, seat_ids: &[String]) -> Result<Vec<usize>, String> {
        seat_ids
            .iter()
            .map(|id| {
                sector
                    .index_of(id)
                    .ok_or_else(|| format!("Seat {} does not exist in sector {}", id, sector.code))
            })
            .collect()
    }

    // ----------------------- Admisión general -----------------------------

    /// Reserva `n` plazas: resta `n` al contador solo si quedan suficientes.
    /// `fetch_update` es el bucle leer -> calcular -> CAS -> reintentar.
    #[inline(always)]
    fn select_general(sector: &SectorGeneral, n: usize) -> Result<(), String> {
        sector
            .available
            .0
            .try_update(Ordering::AcqRel, Ordering::Acquire, |libres| {
                if libres >= n {
                    Some(libres - n)
                } else {
                    None
                }
            })
            .map(|_| {
                sector.selected.0.fetch_add(n, Ordering::AcqRel);
            })
            .map_err(|_| format!("Not enough seats available in sector {}", sector.code))
    }

    /// Venta definitiva: los IDs salen de un contador, así que son únicos.
    #[inline(always)]
    fn book_general(&self, sector: &SectorGeneral, n: usize) -> Vec<String> {
        let primero = sector.next_ticket.0.fetch_add(n, Ordering::AcqRel);
        // Usa directamente los IDs precreados del Venue original (cero format!)
        let tickets = if primero + n <= sector.ids.len() {
            sector.ids[primero..primero + n].to_vec()
        } else {
            (primero + 1..=primero + n)
                .map(|i| format!("{}-{}", sector.code, i))
                .collect()
        };
        // fetch_sub seguro: sin CAS loop, simplemente bajamos selected en min(cur, n)
        let cur_sel = sector.selected.0.load(Ordering::Relaxed);
        sector.selected.0.fetch_sub(cur_sel.min(n), Ordering::AcqRel);
        sector.booked.0.fetch_add(n, Ordering::AcqRel);
        // El total global se actualiza lo último: `remaining_seats()` nunca
        // llega a 0 antes de que la venta esté hecha de verdad.
        self.shared.total_booked.0.fetch_add(n, Ordering::AcqRel);
        tickets
    }

    // ----------------------- Sectores numerados ---------------------------

}

impl Booker for ConcurrentBooker {
    fn get_venue_name(&self) -> &str {
        &self.shared.name
    }

    fn get_total_capacity(&self) -> usize {
        self.shared.capacity
    }

    fn get_total_available_seats(&self) -> usize {
        self.shared.sectors.iter().map(|s| s.available()).sum()
    }

    fn get_available_sectors(&self) -> Vec<String> {
        // Relaxed basta: solo necesitamos saber si hay plazas aprox.,
        // el CAS posterior garantiza la atomicidad real.
        self.shared
            .sectors
            .iter()
            .filter(|s| s.available_relaxed() > 0)
            .map(|s| s.code().to_string())
            .collect()
    }

    fn is_sector_numbered(&self, sector_code: &str) -> bool {
        self.shared
            .sector(sector_code)
            .map(|s| s.is_numbered())
            .unwrap_or(false)
    }

    fn get_available_seats(&self, sector_code: &str) -> Vec<String> {
        match self.shared.sector(sector_code) {
            // En admisión general tomamos las primeras n etiquetas directamente
            // del vector de IDs original (cero format!, cero allocations).
            Some(SectorConcurrente::General(s)) => {
                let n = s.available.get().min(config::MAX_TICKETS);
                if n <= s.ids.len() {
                    s.ids[..n].to_vec()
                } else {
                    (1..=n).map(|i| format!("{}-{}", s.code, i)).collect()
                }
            }
            // Para sectores numerados solo necesitamos MAX_TICKETS*4 candidatos:
            // simulate_booking llama a choose_multiple(n_tickets) sobre este Vec,
            // así que con 24 candidatos hay suficiente variedad sin clonar hasta 300 strings.
            // Empezamos por un índice aleatorio para repartir la contención CAS
            // entre hilos: sin esto todos compiten por los mismos asientos bajos.
            // Usamos Relaxed porque es una lectura de observación; el CAS posterior
            // garantiza la atomicidad real de la reserva.
            Some(SectorConcurrente::Numerado(s)) => {
                let limit = (config::MAX_TICKETS * 4).min(s.capacity);
                let start = if s.capacity > 1 {
                    rand::rng().random_range(0..s.capacity)
                } else {
                    0
                };
                s.seats
                    .iter()
                    .zip(&s.ids)
                    .cycle()
                    .skip(start)
                    .take(s.capacity)
                    .filter(|(estado, _)| estado.load(Ordering::Relaxed) == LIBRE)
                    .map(|(_, id)| id.clone())
                    .take(limit)
                    .collect()
            }
            None => panic!("Unknown sector: '{sector_code}'"),
        }
    }

    fn remaining_seats(&self) -> usize {
        // Relaxed basta: solo se usa para la condición del while loop,
        // no necesita ser un valor exacto, solo "¿queda algo?"
        self.shared.capacity - self.shared.total_booked.0.load(Ordering::Relaxed)
    }

    fn select_sector(&mut self, _sector_code: &str) {
        // No hace falta ninguna acción.
    }

    fn unselect_sector(&mut self, _sector_code: &str) {
        // No hace falta ninguna acción.
    }

    fn deselect_seats(&mut self, sector_code: &str, seat_ids: &[String]) -> Result<(), String> {
        match self.sector(sector_code)? {
            SectorConcurrente::General(s) => {
                let n = seat_ids.len();
                // Decrementamos selected de forma segura (sin pasar de 0)
                // y restauramos available directamente: siempre devolvemos n plazas.
                let cur_sel = s.selected.0.load(Ordering::Relaxed);
                s.selected.0.fetch_sub(cur_sel.min(n), Ordering::AcqRel);
                s.available.0.fetch_add(n, Ordering::AcqRel);
                Ok(())
            }
            SectorConcurrente::Numerado(s) => {
                let idxs = Self::indices(s, seat_ids)?;
                cas::cambiar_todas(s, &idxs, SELECCIONADA, LIBRE)
                    .map_err(|i| format!("Seat {} is not selected", s.ids[i]))?;
                s.available.0.fetch_add(idxs.len(), Ordering::AcqRel);
                Ok(())
            }
        }
    }

    fn select_seats(&mut self, sector_code: &str, seat_ids: &[String]) -> Result<(), String> {
        match self.sector(sector_code)? {
            SectorConcurrente::General(s) => Self::select_general(s, seat_ids.len()),
            SectorConcurrente::Numerado(s) => {
                let mut idxs = Self::indices(s, seat_ids)?;
                idxs.sort_unstable();
                cas::cambiar_todas(s, &idxs, LIBRE, SELECCIONADA)
                    .map_err(|i| format!("Seat {} is not available", s.ids[i]))?;
                s.available.0.fetch_sub(idxs.len(), Ordering::AcqRel);
                Ok(())
            }
        }
    }

    fn book_seats(&mut self, sector_code: &str, seat_ids: &[String]) -> Result<Vec<String>, String> {
        match self.sector(sector_code)? {
            SectorConcurrente::General(s) => Ok(self.book_general(s, seat_ids.len())),
            SectorConcurrente::Numerado(s) => {
                let idxs = Self::indices(s, seat_ids)?;
                match cas::cambiar_todas(s, &idxs, SELECCIONADA, VENDIDA) {
                    Ok(()) => {
                        s.booked.0.fetch_add(idxs.len(), Ordering::AcqRel);
                        self.shared.total_booked.0.fetch_add(idxs.len(), Ordering::AcqRel);
                        Ok(seat_ids.to_vec())
                    }
                    Err(i) => {
                        // Safety net: simulation.rs no llama a deselect_seats
                        // cuando book_seats falla, así que liberamos aquí los
                        // asientos que sigan en SELECCIONADA para no perderlos.
                        for &idx in &idxs {
                            if s.seats[idx]
                                .compare_exchange(SELECCIONADA, LIBRE, Ordering::AcqRel, Ordering::Acquire)
                                .is_ok()
                            {
                                s.available.0.fetch_add(1, Ordering::AcqRel);
                            }
                        }
                        Err(format!("Seat {} must be selected before booking", s.ids[i]))
                    }
                }
            }
        }
    }

    fn get_venue(&self) -> venue::Venue {
        self.shared.snapshot()
    }
}
