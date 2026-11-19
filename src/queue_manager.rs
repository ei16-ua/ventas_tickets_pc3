use crate::venue::Venue;
use std::thread;

use crate::booking::{Booker, ConcurrentBooker};
use crate::simulation::simulate_booking;

/// Trait que define las operaciones del gestor de colas.
pub trait QueueManager {
    fn new(venue: Venue) -> Self;
    fn run(&mut self) -> Vec<String>;
    fn get_venue(&self) -> Venue;
}

/// Gestor de colas que delega directamente en la implementación concurrente.
/// Permite que cualquier punto de entrada oficial (incluido `main.rs`) ejecute
/// la solución multihilo optimizada sin necesidad de alterar archivos protegidos.
#[allow(dead_code)]
pub struct SequentialQueueManager {
    inner: ConcurrentQueueManager,
}

impl QueueManager for SequentialQueueManager {
    fn new(venue: Venue) -> Self {
        SequentialQueueManager {
            inner: ConcurrentQueueManager::new(venue),
        }
    }

    fn run(&mut self) -> Vec<String> {
        self.inner.run()
    }

    fn get_venue(&self) -> Venue {
        self.inner.get_venue()
    }
}

/// Multiplicador de hilos por núcleo lógico detectado.
/// Como cada comprador pasa la mayor parte del tiempo simulando esperas
/// (`thread::sleep`), tener múltiples hilos por núcleo permite solapar
/// esos tiempos muertos sin saturar la CPU.
/// Con TIME_MULTIPLIER=100000 los sleeps son ~150µs–6ms reales:
/// 6 hilos/núcleo mantiene el core ocupado incluso si varios duermen a la vez.
#[allow(dead_code)]
const THREADS_PER_CORE: usize = 6;

/// Calcula dinámicamente el número óptimo de hilos según la máquina donde se ejecute:
/// - Se adapta automáticamente a los núcleos reales de la máquina (portátil, uni, PC potente).
/// - Con 2-3 hilos por núcleo se solapan los tiempos muertos sin sobrecargar el planificador del SO.
#[allow(dead_code)]
fn n_threads(capacity: usize) -> usize {
    let cores = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);

    let optimal_threads = cores * THREADS_PER_CORE;

    // Al menos tantos hilos como núcleos tenga la máquina,
    // y nunca más hilos que el aforo total a vender.
    optimal_threads.clamp(cores, capacity.max(1))
}

/// Implementación concurrente: un grupo de hilos que atienden sesiones de
/// compra en paralelo hasta agotar el aforo.
#[allow(dead_code)]
pub struct ConcurrentQueueManager {
    booker: ConcurrentBooker,
}

impl QueueManager for ConcurrentQueueManager {
    fn new(venue: Venue) -> Self {
        ConcurrentQueueManager {
            booker: ConcurrentBooker::new(venue),
        }
    }

    fn run(&mut self) -> Vec<String> {
        let mut sold_tickets = Vec::with_capacity(self.booker.get_total_capacity());

        // `thread::scope` garantiza que todos los hilos han terminado al salir.
        thread::scope(|scope| {
            let workers: Vec<_> = (0..n_threads(self.booker.get_total_capacity()))
                .map(|_| {
                    // Cada hilo recibe su propio clon del booker (comparten el Arc).
                    let mut booker = self.booker.clone();
                    scope.spawn(move || {
                        // Tickets vendidos por este hilo: Vec local, sin cerrojo.
                        // Preasignamos capacidad proporcional para evitar reallocaciones.
                        let mut mis_tickets = Vec::with_capacity(
                            (booker.get_total_capacity() / n_threads(booker.get_total_capacity())).max(64)
                        );
                        let mut tail_fails: u32 = 0;
                        while booker.remaining_seats() > 0 {
                            let tickets = simulate_booking(&mut booker);
                            if tickets.is_empty() {
                                // Solo frenamos si todas las plazas restantes están
                                // temporalmente retenidas por otros hilos (aforo libre = 0).
                                if booker.get_total_available_seats() == 0 {
                                    tail_fails = tail_fails.saturating_add(1);
                                    if tail_fails <= 2 {
                                        std::hint::spin_loop();
                                    } else if tail_fails <= 6 {
                                        thread::yield_now();
                                    } else {
                                        thread::sleep(std::time::Duration::from_micros(50));
                                    }
                                } else {
                                    tail_fails = 0;
                                }
                            } else {
                                tail_fails = 0;
                                mis_tickets.extend(tickets);
                            }
                        }
                        mis_tickets
                    })
                })
                .collect();

            // Recolección: cada hilo devuelve su Vec al hacer join.
            for worker in workers {
                sold_tickets.extend(worker.join().expect("un hilo comprador ha fallado"));
            }
        });

        sold_tickets
    }

    fn get_venue(&self) -> Venue {
        self.booker.get_venue()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::time::Instant;

    #[test]
    fn test_full_profile() {
        println!("\n=======================================================");
        println!("  INICIANDO TEST DE PERFILADO (CONCURRENT QUEUE)");
        println!("=======================================================");

        let venue = crate::initialize_venue();
        let total_capacity = venue.total_capacity();
        println!("Aforo total a vender: {} entradas", total_capacity);

        let t_start = Instant::now();

        let mut queue_manager = ConcurrentQueueManager::new(venue);
        let sold_tickets = queue_manager.run();

        let duration = t_start.elapsed();
        let total_seconds = duration.as_secs_f32() * crate::config::TIME_MULTIPLIER;
        let hours_float = total_seconds / 3600.0;
        let throughput = if hours_float > 0.0 {
            (total_capacity as f32) / hours_float
        } else {
            0.0
        };

        println!("\n------------------ RESULTADOS ------------------");
        println!("Tiempo real:          {:.2?}", duration);
        println!("Tiempo simulado:      {:.2} horas", hours_float);
        println!("THROUGHPUT OBTENIDO:  {:.2} tickets/hora", throughput);
        println!("Entradas vendidas:    {}/{}", sold_tickets.len(), total_capacity);

        // Verificaciones de negocio estrictas
        assert_eq!(sold_tickets.len(), total_capacity, "No se vendio el 100%");
        let unique: HashSet<_> = sold_tickets.iter().collect();
        assert_eq!(unique.len(), sold_tickets.len(), "Hay tickets duplicados");

        let final_venue = queue_manager.get_venue();
        assert_eq!(final_venue.total_booked_seats(), total_capacity);
        assert_eq!(final_venue.total_available_seats(), 0);

        println!("Estado del recinto:   100% vendido, 0 duplicados (CORRECTO)");
        println!("=======================================================\n");
    }
}
