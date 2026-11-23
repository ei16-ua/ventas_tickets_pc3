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
pub trait Booker {
    #[allow(dead_code)]
    fn get_venue_name(&self) -> &str;
    fn get_total_capacity(&self) -> usize;
    #[allow(dead_code)]
    fn get_total_available_seats(&self) -> usize;
    fn get_available_sectors(&self) -> Vec<String>;
    #[allow(dead_code)]
    fn is_sector_numbered(&self, sector_code: &str) -> bool;
    fn get_available_seats(&self, sector_code: &str) -> Vec<String>;
    fn remaining_seats(&self) -> usize;
    fn select_sector(&mut self, sector_code: &str);
    fn unselect_sector(&mut self, sector_code: &str);
    fn deselect_seats(&mut self, sector_code: &str, seat_ids: &[String]) -> Result<(), String>;
    fn select_seats(&mut self, sector_code: &str, seat_ids: &[String]) -> Result<(), String>;
    fn book_seats(&mut self, sector_code: &str, seat_ids: &[String]) -> Result<Vec<String>, String>;
    fn get_venue(&self) -> venue::Venue;
}

/// Implementación secuencial de referencia del trait `Booker`.
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
            sector
                .get_available_seats()
                .iter()
                .map(|s| s.id.clone())
                .collect()
        } else {
            panic!("Unknown sector: '{sector_code}'");
        }
    }

    fn remaining_seats(&self) -> usize {
        self.venue.total_capacity() - self.venue.total_booked_seats()
    }

    fn select_sector(&mut self, _sector_code: &str) {}
    fn unselect_sector(&mut self, _sector_code: &str) {}

    fn deselect_seats(&mut self, sector_code: &str, seat_ids: &[String]) -> Result<(), String> {
        let sector = self
            .venue
            .sectors
            .iter_mut()
            .find(|s| s.code == sector_code)
            .ok_or_else(|| format!("Unknown sector: '{sector_code}'"))?;

        for seat_id in seat_ids {
            match sector.seats.iter().find(|s| s.id == *seat_id) {
                Some(seat) if seat.state == venue::SeatState::Selected => {}
                Some(_) => return Err(format!("Seat {} is not selected", seat_id)),
                None => {
                    return Err(format!(
                        "Seat {} does not exist in sector {}",
                        seat_id, sector_code
                    ))
                }
            }
        }

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

        for seat_id in seat_ids {
            match sector.seats.iter().find(|s| s.id == *seat_id) {
                Some(seat) if seat.state == venue::SeatState::Available => {}
                Some(_) => return Err(format!("Seat {} is not available", seat_id)),
                None => {
                    return Err(format!(
                        "Seat {} does not exist in sector {}",
                        seat_id, sector_code
                    ))
                }
            }
        }

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

        for seat_id in seat_ids {
            match sector.seats.iter().find(|s| s.id == *seat_id) {
                Some(seat) if seat.state == venue::SeatState::Selected => {}
                Some(seat) if seat.state == venue::SeatState::Booked => {
                    return Err(format!("Seat {} is already booked", seat_id));
                }
                Some(_) => {
                    return Err(format!("Seat {} must be selected before booking", seat_id));
                }
                None => {
                    return Err(format!(
                        "Seat {} does not exist in sector {}",
                        seat_id, sector_code
                    ))
                }
            }
        }

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

    fn indices(sector: &SectorNumerado, seat_ids: &[String]) -> Result<Vec<usize>, String> {
        let mut idxs = Vec::with_capacity(seat_ids.len());
        for id in seat_ids {
            let idx = sector
                .index_of(id)
                .ok_or_else(|| format!("Seat {} does not exist in sector {}", id, sector.code))?;
            idxs.push(idx);
        }
        Ok(idxs)
    }

    // ----------------------- Admisión general -----------------------------

    /// Reserva `n` plazas atómicamente solo si quedan suficientes.
    #[inline(always)]
    fn select_general(sector: &SectorGeneral, n: usize) -> Result<(), String> {
        if n == 0 {
            return Ok(());
        }
        sector
            .available
            .0
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |libres| {
                (libres >= n).then_some(libres - n)
            })
            .map(|_| {
                sector.selected.0.fetch_add(n, Ordering::AcqRel);
            })
            .map_err(|_| format!("Not enough seats available in sector {}", sector.code))
    }

    /// Libera `n` plazas reservadas verificando que sigan en estado `Selected`.
    #[inline(always)]
    fn deselect_general(sector: &SectorGeneral, n: usize) -> Result<(), String> {
        if n == 0 {
            return Ok(());
        }
        sector
            .selected
            .0
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |sel| {
                (sel >= n).then_some(sel - n)
            })
            .map(|_| {
                sector.available.0.fetch_add(n, Ordering::AcqRel);
            })
            .map_err(|_| format!("Not enough selected seats in sector {}", sector.code))
    }

    /// Venta definitiva verificando que existan `n` plazas en estado `Selected`.
    #[inline(always)]
    fn book_general(&self, sector: &SectorGeneral, n: usize) -> Result<Vec<String>, String> {
        if n == 0 {
            return Ok(Vec::new());
        }
        sector
            .selected
            .0
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |sel| {
                (sel >= n).then_some(sel - n)
            })
            .map_err(|_| format!("Seats must be selected before booking in sector {}", sector.code))?;

        let primero = sector.next_ticket.0.fetch_add(n, Ordering::AcqRel);
        let tickets = sector.ids[primero..primero + n].to_vec();

        sector.booked.0.fetch_add(n, Ordering::AcqRel);
        // El total global se actualiza al final: `remaining_seats()` nunca
        // llega a 0 antes de consolidar la venta.
        self.shared.total_booked.0.fetch_add(n, Ordering::AcqRel);
        Ok(tickets)
    }
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
        let mut available = Vec::with_capacity(self.shared.sectors.len());
        for s in &self.shared.sectors {
            if s.available_relaxed() > 0 {
                available.push(s.code().to_string());
            }
        }
        available
    }

    fn is_sector_numbered(&self, sector_code: &str) -> bool {
        self.shared
            .sector(sector_code)
            .map(|s| s.is_numbered())
            .unwrap_or(false)
    }

    fn get_available_seats(&self, sector_code: &str) -> Vec<String> {
        match self.shared.sector(sector_code) {
            Some(SectorConcurrente::General(s)) => {
                let n = s.available.get().min(config::MAX_TICKETS).min(s.ids.len());
                s.ids[..n].to_vec()
            }
            Some(SectorConcurrente::Numerado(s)) => {
                if s.capacity <= 512 {
                    let mut available =
                        Vec::with_capacity(s.available.get_relaxed().min(s.capacity));
                    for i in 0..s.capacity {
                        if s.seats[i].load(Ordering::Relaxed) == LIBRE {
                            available.push(s.ids[i].clone());
                        }
                    }
                    available
                } else {
                    let limit = (config::MAX_TICKETS * 16).min(s.capacity);
                    let start = rand::rng().random_range(0..s.capacity);
                    let step = [7, 13, 19, 29, 37, 53]
                        .into_iter()
                        .find(|&p| s.capacity % p != 0)
                        .unwrap_or(1);
                    let mut available = Vec::with_capacity(limit);
                    let mut idx = start;
                    for _ in 0..s.capacity {
                        if s.seats[idx].load(Ordering::Relaxed) == LIBRE {
                            available.push(s.ids[idx].clone());
                            if available.len() == limit {
                                break;
                            }
                        }
                        idx += step;
                        if idx >= s.capacity {
                            idx -= s.capacity;
                        }
                    }
                    available
                }
            }
            None => panic!("Unknown sector: '{sector_code}'"),
        }
    }

    fn remaining_seats(&self) -> usize {
        self.shared
            .capacity
            .saturating_sub(self.shared.total_booked.0.load(Ordering::Acquire))
    }

    fn select_sector(&mut self, _sector_code: &str) {}
    fn unselect_sector(&mut self, _sector_code: &str) {}

    fn deselect_seats(&mut self, sector_code: &str, seat_ids: &[String]) -> Result<(), String> {
        match self.sector(sector_code)? {
            SectorConcurrente::General(s) => Self::deselect_general(s, seat_ids.len()),
            SectorConcurrente::Numerado(s) => {
                if seat_ids.is_empty() {
                    return Ok(());
                }
                let mut idxs = Self::indices(s, seat_ids)?;
                idxs.sort_unstable();
                cas::cambiar_todas(s, &idxs, SELECCIONADA, LIBRE)
                    .map_err(|i| format!("Seat {} is not selected", s.ids[i]))?;
                s.available.add(idxs.len());
                Ok(())
            }
        }
    }

    fn select_seats(&mut self, sector_code: &str, seat_ids: &[String]) -> Result<(), String> {
        match self.sector(sector_code)? {
            SectorConcurrente::General(s) => Self::select_general(s, seat_ids.len()),
            SectorConcurrente::Numerado(s) => {
                if seat_ids.is_empty() {
                    return Ok(());
                }
                let mut idxs = Self::indices(s, seat_ids)?;
                idxs.sort_unstable();
                cas::cambiar_todas(s, &idxs, LIBRE, SELECCIONADA)
                    .map_err(|i| format!("Seat {} is not available", s.ids[i]))?;
                s.available.sub(idxs.len());
                Ok(())
            }
        }
    }

    fn book_seats(&mut self, sector_code: &str, seat_ids: &[String]) -> Result<Vec<String>, String> {
        match self.sector(sector_code)? {
            SectorConcurrente::General(s) => self.book_general(s, seat_ids.len()),
            SectorConcurrente::Numerado(s) => {
                if seat_ids.is_empty() {
                    return Ok(Vec::new());
                }
                let mut idxs = Self::indices(s, seat_ids)?;
                idxs.sort_unstable();
                cas::cambiar_todas(s, &idxs, SELECCIONADA, VENDIDA)
                    .map_err(|i| format!("Seat {} must be selected before booking", s.ids[i]))?;
                self.shared
                    .total_booked
                    .0
                    .fetch_add(idxs.len(), Ordering::AcqRel);
                Ok(seat_ids.to_vec())
            }
        }
    }

    fn get_venue(&self) -> venue::Venue {
        self.shared.snapshot()
    }
}