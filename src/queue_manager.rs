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

/// Parámetros de concurrencia.
/// Como `simulate_booking` pasa la mayor parte del tiempo en `thread::sleep`,
/// lanzar varios hilos por núcleo solapa las esperas sin saturar la CPU.
const MAX_THREADS: usize = 512;
const MIN_TAIL_WORKERS: usize = 16;

fn n_threads(capacity: usize) -> usize {
    (capacity / 4)
        .max(capacity.min(MIN_TAIL_WORKERS))
        .min(MAX_THREADS)
        .max(1)
}

/// Implementación concurrente: pool de hilos que atienden sesiones de compra
/// en paralelo hasta agotar el aforo.
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
        let total_cap = self.booker.get_total_capacity();
        if total_cap == 0 {
            return Vec::new();
        }

        let num_workers = n_threads(total_cap);
        let min_active = MIN_TAIL_WORKERS.min(num_workers);
        let mut sold_tickets = Vec::with_capacity(total_cap);
        let per_worker_cap = (total_cap / num_workers).max(64);

        thread::scope(|scope| {
            let workers: Vec<_> = (0..num_workers)
                .map(|worker_id| {
                    let mut booker = self.booker.clone();
                    scope.spawn(move || {
                        let mut mis_tickets = Vec::with_capacity(per_worker_cap);
                        loop {
                            let left = booker.remaining_seats();
                            if left == 0 {
                                break;
                            }

                            // Reducción progresiva de hilos activos en la cola final:
                            // evita la estampida de cientos de hilos sin caer a 1 solo hilo.
                            let active_workers = left
                                .saturating_mul(4)
                                .clamp(min_active, num_workers);
                            if worker_id >= active_workers {
                                thread::sleep(std::time::Duration::from_micros(200));
                                continue;
                            }

                            let tickets = simulate_booking(&mut booker);
                            if !tickets.is_empty() {
                                mis_tickets.extend(tickets);
                            }
                        }
                        mis_tickets
                    })
                })
                .collect();

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
    use crate::config;
    use crate::venue::Sector;
    use std::collections::HashSet;
    use std::time::Instant;

    fn build_sample_venue() -> Venue {
        let mut sectors = vec![
            Sector::new("Standing", "ST", config::STANDING_CAPACITY, false),
            Sector::new("Golden Circle", "GC", config::GOLDEN_CIRCLE_CAPACITY, false),
        ];
        for i in 0..config::N_SECTORS {
            sectors.push(Sector::new(
                &format!("Seated {}", i + 1),
                &format!("S{}", i + 1),
                config::SEATED_SECTOR_CAPACITY,
                true,
            ));
        }
        Venue {
            name: String::from("Recinto de Ejemplo"),
            sectors,
        }
    }

    #[test]
    fn test_full_profile() {
        println!("\n=======================================================");
        println!("  INICIANDO TEST DE PERFILADO (CONCURRENT QUEUE)");
        println!("=======================================================");

        let venue = build_sample_venue();
        let total_capacity = venue.total_capacity();
        println!("Aforo total a vender: {} entradas", total_capacity);

        let t_start = Instant::now();

        let mut queue_manager = ConcurrentQueueManager::new(venue);
        let sold_tickets = queue_manager.run();

        let duration = t_start.elapsed();
        let total_seconds = duration.as_secs_f32() * config::TIME_MULTIPLIER;
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