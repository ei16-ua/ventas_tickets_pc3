use std::fmt;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

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
///
/// Cada sector tiene un nombre descriptivo, un código corto (p. ej. `"ST"`, `"S1"`),
/// un vector de asientos (`Seat`), una capacidad total y una bandera que indica
/// si los asientos son numerados (`is_numbered: true`) o de admisión general (`false`).
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
    /// Asiento libre, disponible para selección.
    Available,
    /// Asiento bloqueado provisionalmente por una sesión de compra.
    Selected,
    /// Asiento vendido definitivamente.
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

    /// Devuelve referencias a todos los asientos que se encuentran actualmente disponibles.
    pub fn get_available_seats(&self) -> Vec<&Seat> {
        self.seats.iter().filter(|s| s.state == SeatState::Available).collect()
    }

    /// Devuelve el número de asientos actualmente en estado `Available`.
    pub fn n_available_seats(&self) -> usize {
        self.seats.iter().filter(|s| s.state == SeatState::Available).count()
    }

    /// Devuelve el número de asientos actualmente en estado `Booked`.
    pub fn n_booked_seats(&self) -> usize {
        self.seats.iter().filter(|s| s.state == SeatState::Booked).count()
    }
}

// ===========================================================================
//  MODELO CONCURRENTE
//  Todo el estado que cambia durante la venta está en atómicos, así que se
//  puede compartir entre hilos con un `Arc` sin ningún `Mutex`.
//  El `Venue` de arriba se sigue usando como "foto": se recibe al principio
//  (`from_venue`) y se reconstruye al final (`snapshot`).
// ===========================================================================

/// Estados de una butaca numerada guardados en un `AtomicU8`.
pub const LIBRE: u8 = 0;
pub const SELECCIONADA: u8 = 1;
pub const VENDIDA: u8 = 2;

/// Contador atómico alineado a 64 bytes (una línea de caché).
/// Así dos contadores distintos nunca comparten línea y no hay
/// falso compartido entre sectores.
#[repr(align(64))]
pub struct Contador(pub AtomicUsize);

impl Contador {
    #[allow(dead_code)]
    pub fn new(valor: usize) -> Self {
        Contador(AtomicUsize::new(valor))
    }

    /// Lectura del valor actual (con barrera Acquire).
    pub fn get(&self) -> usize {
        self.0.load(Ordering::Acquire)
    }

    /// Lectura relajada (para observaciones no críticas en el hot path).
    #[allow(dead_code)]
    #[inline(always)]
    pub fn get_relaxed(&self) -> usize {
        self.0.load(Ordering::Relaxed)
    }
}

/// Sector de admisión general (sin butaca concreta): solo importa cuántas
/// plazas quedan, así que basta con contadores.
#[allow(dead_code)]
pub struct SectorGeneral {
    pub name: String,
    pub code: String,
    pub capacity: usize,
    /// Plazas libres (ni seleccionadas ni vendidas).
    pub available: Contador,
    /// Plazas bloqueadas provisionalmente por compradores.
    pub selected: Contador,
    /// Plazas vendidas definitivamente.
    pub booked: Contador,
    /// Siguiente número de ticket a emitir (genera IDs únicos "ST-1", "ST-2"...).
    pub next_ticket: Contador,
    /// Identificadores de tickets tomados del Venue original (evita allocaciones de strings).
    pub ids: Vec<String>,
}

/// Sector numerado: cada butaca tiene su propio estado atómico.
#[allow(dead_code)]
pub struct SectorNumerado {
    pub name: String,
    pub code: String,
    pub capacity: usize,
    /// IDs de las butacas ("S3-1", "S3-2"...). Solo lectura: no necesita sincronización.
    pub ids: Vec<String>,
    /// Estado de cada butaca: LIBRE, SELECCIONADA o VENDIDA.
    pub seats: Vec<AtomicU8>,
    pub available: Contador,
    pub booked: Contador,
}

#[allow(dead_code)]
impl SectorNumerado {
    /// Traduce un ID de butaca ("S3-17") a su posición en `seats` (16).
    /// Fast path O(1): lee el número tras el último '-'. Los IDs siempre
    /// siguen el formato "CODE-N" generado por Sector::new, así que
    /// basta verificar que el índice esté en rango.
    #[inline(always)]
    pub fn index_of(&self, seat_id: &str) -> Option<usize> {
        let numero: usize = seat_id.rsplit('-').next()?.parse().ok()?;
        let idx = numero.checked_sub(1)?;
        if idx < self.capacity {
            Some(idx)
        } else {
            None
        }
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

    /// Lectura relajada de plazas disponibles (solo para filtrados de observación).
    pub fn available_relaxed(&self) -> usize {
        match self {
            SectorConcurrente::General(s) => s.available.0.load(Ordering::Relaxed),
            SectorConcurrente::Numerado(s) => s.available.0.load(Ordering::Relaxed),
        }
    }
}

/// Recinto compartido entre todos los hilos (dentro de un `Arc`).
#[allow(dead_code)]
pub struct ConcurrentVenue {
    pub name: String,
    pub sectors: Vec<SectorConcurrente>,
    pub capacity: usize,
    /// Total de entradas vendidas en todo el recinto.
    pub total_booked: Contador,
}

#[allow(dead_code)]
impl ConcurrentVenue {
    /// Construye el estado concurrente a partir del `Venue` inicial.
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
                    available: Contador::new(available),
                    booked: Contador::new(booked),
                }));
            } else {
                sectors.push(SectorConcurrente::General(SectorGeneral {
                    name: s.name.clone(),
                    code: s.code.clone(),
                    capacity: s.capacity,
                    available: Contador::new(available),
                    selected: Contador::new(0),
                    booked: Contador::new(booked),
                    next_ticket: Contador::new(booked),
                    ids: s.seats.iter().map(|seat| seat.id.clone()).collect(),
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

    /// Busca un sector por su código.
    /// Con ≤30 sectores la búsqueda lineal es más rápida que el HashMap
    /// por mejor localidad de caché (no hay indirección de hash ni posible miss).
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
                    // Reutilizamos los IDs precalculados para evitar miles de format!
                    let seats = s
                        .ids
                        .iter()
                        .enumerate()
                        .map(|(idx, id)| {
                            let i = idx + 1;
                            Seat {
                                id: id.clone(),
                                state: if i <= booked {
                                    SeatState::Booked
                                } else if i <= booked + selected {
                                    SeatState::Selected
                                } else {
                                    SeatState::Available
                                },
                            }
                        })
                        .collect();
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
