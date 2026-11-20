use std::fmt;
use std::sync::atomic::{AtomicIsize, AtomicU8, AtomicUsize, Ordering};

/// Modelo de un recinto (*Venue*), compuesto por un nombre y múltiples sectores.
#[derive(Clone)]
pub struct Venue {
    pub name: String,
    pub sectors: Vec<Sector>,
}

impl Venue {
    /// Capacidad total sumando todos los sectores.
    pub fn total_capacity(&self) -> usize {
        self.sectors.iter().map(|s| s.capacity).sum()
    }

    /// Total de asientos disponibles sumando todos los sectores.
    pub fn total_available_seats(&self) -> usize {
        self.sectors.iter().map(|s| s.n_available_seats()).sum()
    }

    /// Total de asientos ya vendidos (*Booked*) sumando todos los sectores.
    pub fn total_booked_seats(&self) -> usize {
        self.sectors.iter().map(|s| s.n_booked_seats()).sum()
    }
}

impl fmt::Display for Venue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {}/{}\n",
            self.name,
            self.total_booked_seats(),
            self.total_capacity(),
        )?;

        write!(f, "Sectors:\n")?;

        for sector in &self.sectors {
            write!(
                f,
                "  - {} ({}) {}/{}\n",
                sector.name,
                sector.code,
                sector.n_booked_seats(),
                sector.capacity,
            )?;
        }
        Ok(())
    }
}

/// Representa un sector dentro del recinto.
#[derive(Clone)]
pub struct Sector {
    pub name: String,
    pub code: String,
    pub seats: Vec<Seat>,
    pub capacity: usize,
    pub is_numbered: bool,
}

/// Estado en el que se encuentra un asiento o localidad.
#[derive(PartialEq, Clone)]
pub enum SeatState {
    Available,
    Selected,
    Booked,
}

/// Representa una localidad individual con su estado e identificador único.
#[derive(Clone)]
pub struct Seat {
    pub state: SeatState,
    pub id: String,
}

impl Sector {
    /// Crea un nuevo sector inicializando todos sus asientos en estado `Available`.
    pub fn new(name: &str, code: &str, capacity: usize, is_numbered: bool) -> Sector {
        let seats = (1..=capacity)
            .map(|i| Seat {
                state: SeatState::Available,
                id: format!("{}-{}", code, i),
            })
            .collect();

        Sector {
            name: name.to_string(),
            code: code.to_string(),
            seats,
            capacity,
            is_numbered,
        }
    }

    pub fn get_available_seats(&self) -> Vec<&Seat> {
        self.seats
            .iter()
            .filter(|s| s.state == SeatState::Available)
            .collect()
    }

    pub fn n_available_seats(&self) -> usize {
        self.seats
            .iter()
            .filter(|s| s.state == SeatState::Available)
            .count()
    }

    pub fn n_booked_seats(&self) -> usize {
        self.seats
            .iter()
            .filter(|s| s.state == SeatState::Booked)
            .count()
    }
}

// ===========================================================================
//  MODELO CONCURRENTE (sin cerrojos)
// ===========================================================================

/// Estados de una butaca numerada guardados en un `AtomicU8`.
pub const LIBRE: u8 = 0;
pub const SELECCIONADA: u8 = 1;
pub const VENDIDA: u8 = 2;

/// Contador atómico sin signo alineado a 64 bytes para evitar false sharing.
#[repr(align(64))]
pub struct Contador(pub AtomicUsize);

impl Contador {
    #[allow(dead_code)]
    pub fn new(valor: usize) -> Self {
        Contador(AtomicUsize::new(valor))
    }

    #[inline(always)]
    pub fn get(&self) -> usize {
        self.0.load(Ordering::Acquire)
    }

    #[allow(dead_code)]
    #[inline(always)]
    pub fn get_relaxed(&self) -> usize {
        self.0.load(Ordering::Relaxed)
    }
}

/// Contador atómico con signo alineado a 64 bytes.
/// Evita underflow transitorio cuando un hilo decrementa antes de que otro
/// complete el incremento tras liberar asientos con CAS.
#[repr(align(64))]
pub struct ContadorIsize(pub AtomicIsize);

impl ContadorIsize {
    #[allow(dead_code)]
    pub fn new(valor: usize) -> Self {
        ContadorIsize(AtomicIsize::new(valor as isize))
    }

    #[inline(always)]
    pub fn get(&self) -> usize {
        self.0.load(Ordering::Acquire).max(0) as usize
    }

    #[inline(always)]
    pub fn get_relaxed(&self) -> usize {
        self.0.load(Ordering::Relaxed).max(0) as usize
    }

    #[inline(always)]
    pub fn add(&self, n: usize) {
        self.0.fetch_add(n as isize, Ordering::AcqRel);
    }

    #[inline(always)]
    pub fn sub(&self, n: usize) {
        self.0.fetch_sub(n as isize, Ordering::AcqRel);
    }
}

/// Sector de admisión general: gestionado mediante contadores atómicos.
#[allow(dead_code)]
pub struct SectorGeneral {
    pub name: String,
    pub code: String,
    pub capacity: usize,
    pub available: Contador,
    pub selected: Contador,
    pub booked: Contador,
    pub next_ticket: Contador,
    /// IDs que ya venían en estado `Booked` en el `Venue` inicial (vacío en el caso normal).
    pub initial_booked_ids: Vec<String>,
    /// IDs disponibles para emitir (todos los no vendidos inicialmente).
    pub ids: Vec<String>,
}

/// Sector numerado: cada butaca tiene su propio estado atómico.
#[allow(dead_code)]
pub struct SectorNumerado {
    pub name: String,
    pub code: String,
    pub capacity: usize,
    pub ids: Vec<String>,
    pub seats: Vec<AtomicU8>,
    pub available: ContadorIsize,
}

#[allow(dead_code)]
impl SectorNumerado {
    /// Traduce un ID de butaca ("S3-17") a su posición en `seats` (16),
    /// verificando que pertenezca a este sector y esté en rango.
    #[inline(always)]
    pub fn index_of(&self, seat_id: &str) -> Option<usize> {
        let resto = seat_id.strip_prefix(&self.code)?.strip_prefix('-')?;
        let numero: usize = resto.parse().ok()?;
        let idx = numero.checked_sub(1)?;
        (idx < self.capacity).then_some(idx)
    }
}

#[allow(dead_code)]
pub enum SectorConcurrente {
    General(SectorGeneral),
    Numerado(SectorNumerado),
}

#[allow(dead_code)]
impl SectorConcurrente {
    pub fn code(&self) -> &str {
        match self {
            SectorConcurrente::General(s) => &s.code,
            SectorConcurrente::Numerado(s) => &s.code,
        }
    }

    pub fn is_numbered(&self) -> bool {
        matches!(self, SectorConcurrente::Numerado(_))
    }

    pub fn available(&self) -> usize {
        match self {
            SectorConcurrente::General(s) => s.available.get(),
            SectorConcurrente::Numerado(s) => s.available.get(),
        }
    }

    pub fn available_relaxed(&self) -> usize {
        match self {
            SectorConcurrente::General(s) => s.available.get_relaxed(),
            SectorConcurrente::Numerado(s) => s.available.get_relaxed(),
        }
    }
}

/// Recinto compartido entre todos los hilos (dentro de un `Arc`).
#[allow(dead_code)]
pub struct ConcurrentVenue {
    pub name: String,
    pub sectors: Vec<SectorConcurrente>,
    pub capacity: usize,
    pub total_booked: Contador,
}

#[allow(dead_code)]
impl ConcurrentVenue {
    pub fn from_venue(venue: &Venue) -> Self {
        let mut sectors = Vec::with_capacity(venue.sectors.len());
        let mut total_booked = 0;

        for s in venue.sectors.iter() {
            let available = s.n_available_seats();
            let booked = s.n_booked_seats();
            total_booked += booked;

            if s.is_numbered {
                let seats = s
                    .seats
                    .iter()
                    .map(|seat| {
                        AtomicU8::new(match seat.state {
                            SeatState::Available => LIBRE,
                            SeatState::Selected => SELECCIONADA,
                            SeatState::Booked => VENDIDA,
                        })
                    })
                    .collect();
                sectors.push(SectorConcurrente::Numerado(SectorNumerado {
                    name: s.name.clone(),
                    code: s.code.clone(),
                    capacity: s.capacity,
                    ids: s.seats.iter().map(|seat| seat.id.clone()).collect(),
                    seats,
                    available: ContadorIsize::new(available),
                }));
            } else {
                let selected = s
                    .seats
                    .iter()
                    .filter(|seat| seat.state == SeatState::Selected)
                    .count();
                let mut initial_booked_ids = Vec::with_capacity(booked);
                let mut ids = Vec::with_capacity(s.seats.len().saturating_sub(booked));
                for seat in &s.seats {
                    if seat.state == SeatState::Booked {
                        initial_booked_ids.push(seat.id.clone());
                    } else {
                        ids.push(seat.id.clone());
                    }
                }
                sectors.push(SectorConcurrente::General(SectorGeneral {
                    name: s.name.clone(),
                    code: s.code.clone(),
                    capacity: s.capacity,
                    available: Contador::new(available),
                    selected: Contador::new(selected),
                    booked: Contador::new(booked),
                    next_ticket: Contador::new(0),
                    initial_booked_ids,
                    ids,
                }));
            }
        }

        ConcurrentVenue {
            name: venue.name.clone(),
            sectors,
            capacity: venue.total_capacity(),
            total_booked: Contador::new(total_booked),
        }
    }

    #[inline(always)]
    pub fn sector(&self, code: &str) -> Option<&SectorConcurrente> {
        self.sectors.iter().find(|s| s.code() == code)
    }

    /// Reconstruye un `Venue` normal con el estado actual (para `get_venue`).
    pub fn snapshot(&self) -> Venue {
        let sectors = self
            .sectors
            .iter()
            .map(|sc| match sc {
                SectorConcurrente::General(s) => {
                    let booked = s.booked.get();
                    let available = s.available.get();
                    let selected = s.capacity.saturating_sub(booked + available);
                    let newly_booked = booked.saturating_sub(s.initial_booked_ids.len());

                    let mut seats = Vec::with_capacity(s.capacity);
                    for id in &s.initial_booked_ids {
                        seats.push(Seat {
                            id: id.clone(),
                            state: SeatState::Booked,
                        });
                    }
                    for (idx, id) in s.ids.iter().enumerate() {
                        seats.push(Seat {
                            id: id.clone(),
                            state: if idx < newly_booked {
                                SeatState::Booked
                            } else if idx < newly_booked + selected {
                                SeatState::Selected
                            } else {
                                SeatState::Available
                            },
                        });
                    }

                    Sector {
                        name: s.name.clone(),
                        code: s.code.clone(),
                        capacity: s.capacity,
                        is_numbered: false,
                        seats,
                    }
                }
                SectorConcurrente::Numerado(s) => Sector {
                    name: s.name.clone(),
                    code: s.code.clone(),
                    capacity: s.capacity,
                    is_numbered: true,
                    seats: s
                        .ids
                        .iter()
                        .zip(&s.seats)
                        .map(|(id, st)| Seat {
                            id: id.clone(),
                            state: match st.load(Ordering::Acquire) {
                                LIBRE => SeatState::Available,
                                SELECCIONADA => SeatState::Selected,
                                _ => SeatState::Booked,
                            },
                        })
                        .collect(),
                },
            })
            .collect();

        Venue {
            name: self.name.clone(),
            sectors,
        }
    }
}