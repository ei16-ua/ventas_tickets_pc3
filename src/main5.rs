#![allow(dead_code)]

mod booking;
mod config;
mod queue_manager;
mod simulation;
mod venue;

use std::collections::HashSet;
use std::mem::{align_of, size_of};
use std::thread;
use std::time::Instant;

use booking::ConcurrentBooker;
use queue_manager::{ConcurrentQueueManager, QueueManager};
use venue::{Contador, ContadorIsize, Sector, Venue};

pub fn initialize_venue() -> Venue {
    let mut sectors: Vec<Sector> = vec![
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

fn print_arm_diagnostics() {
    let arch = std::env::consts::ARCH;
    let os = std::env::consts::OS;
    let family = std::env::consts::FAMILY;
    let cores = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let is_arm = matches!(arch, "aarch64" | "arm");

    println!("================ DIAGNÓSTICO DE PLATAFORMA / ARM ================");
    println!("Arquitectura CPU:      {} (is_arm = {})", arch, is_arm);
    println!("Sistema Operativo:     {} ({})", os, family);
    println!(
        "Endianness / Puntero:  {} / {}-bit",
        if cfg!(target_endian = "little") {
            "little-endian"
        } else {
            "big-endian"
        },
        size_of::<usize>() * 8
    );
    println!("Núcleos lógicos:       {}", cores);
    println!(
        "Alineación Contador:   {} bytes (size = {} bytes)",
        align_of::<Contador>(),
        size_of::<Contador>()
    );
    println!(
        "Alineación Isize:      {} bytes (size = {} bytes)",
        align_of::<ContadorIsize>(),
        size_of::<ContadorIsize>()
    );
    println!(
        "Tamaño ConcurrentBooker: {} bytes (solo puntero Arc)",
        size_of::<ConcurrentBooker>()
    );
    if !is_arm {
        println!(
            "[INFO] Estás ejecutando en '{}'. Los tests validan ordenación de memoria débil\n\
             [INFO] (Acquire/Release/AcqRel), alineación y huella de memoria pensada para\n\
             [INFO] ARM64/Apple Silicon M1. Al correrlo en tu Mac M1/ARM64 verás 'aarch64'.",
            arch
        );
    }
    #[cfg(target_os = "linux")]
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if line.starts_with("VmRSS:")
                || line.starts_with("VmHWM:")
                || line.starts_with("Threads:")
            {
                println!("Proc status:           {}", line.trim());
            }
        }
    }
    println!("=================================================================");
}

fn main() {
    println!("\n========= Verificación de Portabilidad y Concurrencia ARM64 — main5.rs =========");
    print_arm_diagnostics();

    let t0 = Instant::now();
    let venue = initialize_venue();
    let cap = venue.total_capacity();
    let mut qm = ConcurrentQueueManager::new(venue);
    let sold = qm.run();
    let elapsed = t0.elapsed();

    let unique: HashSet<&String> = sold.iter().collect();
    println!(
        "Simulación rápida 22.500 plazas: vendidas={}/{} | únicas={} | tiempo={:.2?}",
        sold.len(),
        cap,
        unique.len(),
        elapsed
    );
    println!("Ejecuta la batería completa con: cargo test --release --bin main5 -- --nocapture\n");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};
    use std::time::Duration;

    use rand::seq::IndexedRandom;
    use rand::Rng;

    use crate::booking::Booker;
    use crate::venue::{ConcurrentVenue, SeatState, SectorConcurrente};

    /// 1. Verifica alineación de caché, tamaños sin sobrecarga y garantías atómicas
    ///    necesarias para ARM64 (Apple M1 8GB y placas ARM ligeras).
    #[test]
    fn t01_arm_architecture_and_memory_layout() {
        print_arm_diagnostics();

        // En 64-bit (aarch64 / x86_64), usize es de 8 bytes y AtomicU8 de 1 byte
        assert!(size_of::<usize>() >= 4);
        assert_eq!(size_of::<AtomicU8>(), 1);
        assert_eq!(align_of::<AtomicU8>(), 1);

        // Protección contra false sharing: al menos 64 bytes
        assert!(
            align_of::<Contador>() >= 64,
            "Contador debe alinearse al menos a 64 bytes para evitar false sharing"
        );
        assert!(align_of::<ContadorIsize>() >= 64);

        // Clonar ConcurrentBooker debe ser tan barato como clonar un Arc (1 puntero)
        assert_eq!(
            size_of::<ConcurrentBooker>(),
            size_of::<Arc<ConcurrentVenue>>(),
            "ConcurrentBooker debe ser un wrapper ligero de Arc"
        );

        // Estimar huella en memoria del recinto oficial (22.500 localidades)
        let venue = initialize_venue();
        let cap = venue.total_capacity();
        let cvenue = ConcurrentVenue::from_venue(&venue);

        let est_ids_bytes: usize = cvenue
            .sectors
            .iter()
            .map(|s| match s {
                SectorConcurrente::General(g) => {
                    g.ids.capacity() * size_of::<String>()
                        + g.ids.iter().map(|id| id.capacity()).sum::<usize>()
                }
                SectorConcurrente::Numerado(n) => {
                    n.ids.capacity() * size_of::<String>()
                        + n.ids.iter().map(|id| id.capacity()).sum::<usize>()
                        + n.seats.capacity()
                }
            })
            .sum();

        println!(
            "Huella estimada ConcurrentVenue ({} entradas): {:.2} KB",
            cap,
            est_ids_bytes as f64 / 1024.0
        );
        // Debe caber holgadamente en menos de 10 MB incluso en ARM muy ligeros
        assert!(
            est_ids_bytes < 10 * 1024 * 1024,
            "Huella de memoria excesiva: {} bytes",
            est_ids_bytes
        );
    }

    /// 2. Test de modelo de memoria débil (ARM64 Weak Memory Ordering Stress Test).
    ///
    /// A diferencia de x86 (TSO), ARM64 puede reordenar lecturas y escrituras entre hilos
    /// si faltan barreras `Acquire`/`Release`. Este test elimina todos los `thread::sleep`
    /// de `simulation.rs` para saturar el bus de memoria con CAS y operaciones atómicas
    /// puras mientras un hilo observador inspecciona invariantes en vivo.
    #[test]
    fn t02_weak_memory_ordering_cas_hammer() {
        for round in 1..=5 {
            let sectors = vec![
                Sector::new("Standing", "ST", 400, false),
                Sector::new("Golden", "GC", 150, false),
                Sector::new("Grada 1", "S1", 120, true),
                Sector::new("Grada 2", "S2", 120, true),
                Sector::new("Grada 3", "S3", 60, true),
            ];
            let venue = Venue {
                name: format!("ARM-WeakMem-R{}", round),
                sectors,
            };
            let total_cap = venue.total_capacity();
            let booker = ConcurrentBooker::new(venue);
            let done = Arc::new(AtomicBool::new(false));
            let workers = 16;
            let barrier = Arc::new(Barrier::new(workers + 1));

            let mut all_tickets = Vec::with_capacity(total_cap);

            thread::scope(|s| {
                // Hilo observador continuo: comprueba que snapshot() y remaining_seats()
                // jamás observan estados imposibles por reordenamiento de memoria.
                let obs_booker = booker.clone();
                let obs_done = Arc::clone(&done);
                let obs_barrier = Arc::clone(&barrier);
                let observer = s.spawn(move || {
                    obs_barrier.wait();
                    let mut checks = 0usize;
                    while !obs_done.load(Ordering::Acquire) {
                        let rem = obs_booker.remaining_seats();
                        assert!(
                            rem <= total_cap,
                            "Ronda {}: remaining_seats ({}) > total_cap ({})",
                            round,
                            rem,
                            total_cap
                        );
                        if checks % 32 == 0 {
                            let snap = obs_booker.get_venue();
                            let booked = snap.total_booked_seats();
                            let avail = snap.total_available_seats();
                            assert!(
                                booked <= total_cap,
                                "Ronda {}: snapshot booked ({}) > total_cap ({})",
                                round,
                                booked,
                                total_cap
                            );
                            assert!(
                                avail <= total_cap,
                                "Ronda {}: snapshot avail ({}) > total_cap ({})",
                                round,
                                avail,
                                total_cap
                            );
                        }
                        checks += 1;
                        thread::yield_now();
                    }
                    checks
                });

                // Hilos compradores sin sleeps (máxima contención de memoria/CAS)
                let handles: Vec<_> = (0..workers)
                    .map(|_| {
                        let mut b = booker.clone();
                        let bar = Arc::clone(&barrier);
                        s.spawn(move || {
                            let mut local_tickets = Vec::new();
                            let mut rng = rand::rng();
                            bar.wait();

                            while b.remaining_seats() > 0 {
                                let avail_sectors = b.get_available_sectors();
                                if avail_sectors.is_empty() {
                                    thread::yield_now();
                                    continue;
                                }
                                let sec = &avail_sectors[rng.random_range(0..avail_sectors.len())];
                                let avail_seats = b.get_available_seats(sec);
                                if avail_seats.is_empty() {
                                    continue;
                                }
                                let max_req = avail_seats.len().min(config::MAX_TICKETS);
                                let n = rng.random_range(1..=max_req);
                                let chosen: Vec<String> =
                                    avail_seats.choose_multiple(&mut rng, n).cloned().collect();

                                if b.select_seats(sec, &chosen).is_ok() {
                                    // 40% cancela, 60% confirma
                                    if rand::random_bool(0.4) {
                                        assert!(
                                            b.deselect_seats(sec, &chosen).is_ok(),
                                            "Ronda {}: deselect falló tras select exitoso",
                                            round
                                        );
                                    } else {
                                        let booked =
                                            b.book_seats(sec, &chosen).unwrap_or_else(|e| {
                                                panic!(
                                                    "Ronda {}: book falló tras select exitoso: {}",
                                                    round, e
                                                )
                                            });
                                        local_tickets.extend(booked);
                                    }
                                }
                            }
                            local_tickets
                        })
                    })
                    .collect();

                for h in handles {
                    all_tickets.extend(h.join().expect("worker panic"));
                }
                done.store(true, Ordering::Release);
                let _checks = observer.join().expect("observer panic");
            });

            // Verificaciones estrictas post-barrera
            assert_eq!(all_tickets.len(), total_cap, "Ronda {}: falta aforo", round);
            let unique: HashSet<&String> = all_tickets.iter().collect();
            assert_eq!(unique.len(), total_cap, "Ronda {}: duplicados", round);

            let fv = booker.get_venue();
            assert_eq!(fv.total_booked_seats(), total_cap);
            assert_eq!(fv.total_available_seats(), 0);
            for sec in &fv.sectors {
                for seat in &sec.seats {
                    assert!(
                        matches!(seat.state, SeatState::Booked),
                        "Ronda {}: asiento {} huérfano",
                        round,
                        seat.id
                    );
                }
            }
        }
        println!("  OK — 5 rondas intensivas de memoria débil ARM64 sin reordenamientos inválidos");
    }

    /// 3. Test específico para `ContadorIsize` bajo sesgo transitorio (CAS release vs sub).
    ///
    /// Fuerza que múltiples hilos liberen asientos numerados (deselect) mientras otros
    /// los capturan al instante siguiente antes de que el hilo liberador sume al contador.
    /// Verifica que no hay fugas, bloqueos ni valores negativos residuales.
    #[test]
    fn t03_contador_isize_transient_skew_and_recovery() {
        let cap = 24;
        let venue = Venue {
            name: "Isize-Skew".to_string(),
            sectors: vec![Sector::new("MicroNumerado", "MN", cap, true)],
        };
        let booker = ConcurrentBooker::new(venue);
        let threads = 16;
        let iters = 400;
        let barrier = Arc::new(Barrier::new(threads));

        thread::scope(|s| {
            for _ in 0..threads {
                let mut b = booker.clone();
                let bar = Arc::clone(&barrier);
                s.spawn(move || {
                    let mut rng = rand::rng();
                    bar.wait();
                    for _ in 0..iters {
                        let avail = b.get_available_seats("MN");
                        if avail.is_empty() {
                            thread::yield_now();
                            continue;
                        }
                        let take = avail.len().min(3);
                        let n = rng.random_range(1..=take);
                        let batch: Vec<String> =
                            avail.choose_multiple(&mut rng, n).cloned().collect();
                        if b.select_seats("MN", &batch).is_ok() {
                            // Liberación inmediata para provocar carrera entre deselect.add y otro select.sub
                            b.deselect_seats("MN", &batch).unwrap();
                        }
                    }
                });
            }
        });

        assert_eq!(
            booker.get_total_available_seats(),
            cap,
            "ContadorIsize desincronizado tras contención extrema"
        );

        // Ahora comprobamos que se pueden vender las 24 butacas limpiamente
        let mut qm = ConcurrentQueueManager::new(booker.get_venue());
        let sold = qm.run();
        assert_eq!(sold.len(), cap);
        let unique: HashSet<String> = sold.into_iter().collect();
        assert_eq!(unique.len(), cap);
        println!(
            "  OK — ContadorIsize estable tras {} ciclos concurrentes",
            threads * iters
        );
    }

    /// 4. Test orientado a líneas de caché de 128 bytes (Apple Silicon M1) y ARM ligero.
    ///
    /// Ejecuta sectores contiguos en memoria golpeados por múltiples hilos a la vez,
    /// y valida además un escenario ligero tipo Raspberry Pi / contenedor ARM con poco aforo.
    #[test]
    fn t04_m1_cacheline_contention_and_lightweight_arm() {
        // A) Contención entre sectores contiguos (para verificar que no hay corrupción por líneas L2 de 128B)
        let mut sectors = Vec::new();
        for i in 1..=8 {
            sectors.push(Sector::new(&format!("GA{}", i), &format!("GA{}", i), 250, false));
            sectors.push(Sector::new(&format!("NU{}", i), &format!("NU{}", i), 250, true));
        }
        let venue = Venue {
            name: "M1-CacheLine-128B".to_string(),
            sectors,
        };
        let cap = venue.total_capacity();
        let t0 = Instant::now();
        let mut qm = ConcurrentQueueManager::new(venue);
        let sold = qm.run();
        let dur = t0.elapsed();

        assert_eq!(sold.len(), cap);
        let unique: HashSet<String> = sold.into_iter().collect();
        assert_eq!(unique.len(), cap);
        assert_eq!(qm.get_venue().total_available_seats(), 0);
        println!(
            "  OK — Escenario 16 sectores contiguos ({} plazas) en {:.2?}",
            cap, dur
        );

        // B) Escenario ultra ligero (simula límites estrictos / pocos asientos sin exceso de hilos)
        for small_cap in [1usize, 2, 5, 16] {
            let small_venue = Venue {
                name: format!("ARM-Lite-{}", small_cap),
                sectors: vec![Sector::new("Lite", "LT", small_cap, small_cap % 2 == 1)],
            };
            let mut qm_small = ConcurrentQueueManager::new(small_venue);
            let sold_small = qm_small.run();
            assert_eq!(sold_small.len(), small_cap);
            let uniq: HashSet<String> = sold_small.into_iter().collect();
            assert_eq!(uniq.len(), small_cap);
        }
        println!("  OK — Escenarios ARM ligeros (1, 2, 5, 16 plazas) completados sin pánicos");
    }

    /// 5. Test de integración completo del recinto oficial (22.500 plazas)
    ///    midiendo latencia real y throughput en la máquina actual (x86 o ARM64).
    #[test]
    fn t05_arm_full_official_venue_benchmark() {
        let venue = initialize_venue();
        let cap = venue.total_capacity();
        let t0 = Instant::now();

        let mut qm = ConcurrentQueueManager::new(venue);
        let sold = qm.run();
        let elapsed = t0.elapsed();

        let sim_hours = (elapsed.as_secs_f32() * config::TIME_MULTIPLIER) / 3600.0;
        let throughput = (cap as f32) / sim_hours.max(1e-6);

        println!(
            "  [{} / {}] 22.500 entradas en {:.2?} reales | Throughput: {:.2} tickets/hora",
            std::env::consts::ARCH,
            std::env::consts::OS,
            elapsed,
            throughput
        );

        assert_eq!(sold.len(), cap);
        let unique: HashSet<String> = sold.into_iter().collect();
        assert_eq!(unique.len(), cap);

        let fv = qm.get_venue();
        assert_eq!(fv.total_booked_seats(), cap);
        assert_eq!(fv.total_available_seats(), 0);
        assert!(
            elapsed < Duration::from_secs(20),
            "Ejecución demasiado lenta ({:.2?}) para el hardware actual",
            elapsed
        );
    }

    /// 6. Verifica que el acceso concurrente directo a `ConcurrentBooker` mantiene
    ///    integridad sin pérdida ni duplicación bajo compras simultáneas directas.
    #[test]
    fn t06_direct_booker_concurrent_safety() {
        let venue = Venue {
            name: "Direct-Safety".to_string(),
            sectors: vec![
                Sector::new("General", "GEN", 100, false),
                Sector::new("Numerado", "NUM", 100, true),
            ],
        };
        let booker = ConcurrentBooker::new(venue);
        let sold_count = Arc::new(AtomicUsize::new(0));

        thread::scope(|s| {
            for t_id in 0..8 {
                let mut b = booker.clone();
                let sc = Arc::clone(&sold_count);
                s.spawn(move || {
                    let sec = if t_id % 2 == 0 { "GEN" } else { "NUM" };
                    while b.remaining_seats() > 0 {
                        let avail = b.get_available_seats(sec);
                        if avail.is_empty() {
                            thread::yield_now();
                            continue;
                        }
                        let batch = vec![avail[0].clone()];
                        if b.select_seats(sec, &batch).is_ok() {
                            let res = b.book_seats(sec, &batch).unwrap();
                            sc.fetch_add(res.len(), Ordering::AcqRel);
                        }
                    }
                });
            }
        });

        assert_eq!(sold_count.load(Ordering::Acquire), 200);
        let fv = booker.get_venue();
        assert_eq!(fv.total_booked_seats(), 200);
        assert_eq!(fv.total_available_seats(), 0);
    }
}
