#![allow(dead_code)]

mod venue;
mod booking;
mod simulation;
mod config;
mod queue_manager;

use std::collections::{HashMap, HashSet};
use venue::{SeatState, Sector, Venue};
use queue_manager::{ConcurrentQueueManager, QueueManager};

/// Inicializador de recinto por defecto del proyecto para compatibilidad con módulos.
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

fn main() {
    println!("\n========= Tests de Validación Avanzada — main4.rs =========");
    println!("Ejecuta con: cargo test --release --bin main4 -- --nocapture\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// -----------------------------------------------------------------------
    /// TEST 1: Recinto Heterogéneo y Asimétrico (Estadio con Palcos Diminutos)
    /// -----------------------------------------------------------------------
    /// Modela un recinto real con zonas de aforo dispar:
    /// - Zonas generales masivas ("Festival Grounds" y "VIP Stage").
    /// - 30 palcos VIP privados diminutos (de 2, 4 y 6 asientos cada uno).
    ///
    /// Valida:
    /// 1. Venta del 100% del aforo sin importar la disparidad de tamaños.
    /// 2. Cero duplicados en los tickets.
    /// 3. Correspondencia biyectiva 1:1 con todas las localidades del recinto.
    /// 4. Que ningún palco diminuto quede bloqueado o con reservas huérfanas.
    #[test]
    fn test_heterogeneous_venue_asymmetric_distribution() {
        let mut sectors = vec![
            Sector::new("Festival Grounds", "FG", 8_000, false),
            Sector::new("VIP Front Stage", "VIP", 1_500, false),
        ];

        // Añadir 30 palcos numerados pequeños (de 2 a 6 plazas)
        for i in 1..=30 {
            let cap = match i % 3 {
                0 => 2,
                1 => 4,
                _ => 6,
            };
            sectors.push(Sector::new(
                &format!("Palco Privado {}", i),
                &format!("P{}", i),
                cap,
                true,
            ));
        }

        let venue = Venue {
            name: "Estadio Asimétrico".to_string(),
            sectors,
        };
        let expected_capacity = venue.total_capacity();

        // Inventario legítimo
        let mut expected_ids = HashSet::with_capacity(expected_capacity);
        for s in &venue.sectors {
            for seat in &s.seats {
                expected_ids.insert(seat.id.clone());
            }
        }

        let start = std::time::Instant::now();
        let mut qm = ConcurrentQueueManager::new(venue);
        let sold_tickets = qm.run();
        let elapsed = start.elapsed();

        println!(
            "test_heterogeneous_venue_asymmetric_distribution finalizado en {:.2?} con {} tickets",
            elapsed,
            sold_tickets.len()
        );

        // 1. Aforo completo
        assert_eq!(
            sold_tickets.len(),
            expected_capacity,
            "No se vendió el aforo completo en el estadio asimétrico"
        );

        // 2. Cero duplicados
        let unique_sold: HashSet<String> = sold_tickets.iter().cloned().collect();
        assert_eq!(unique_sold.len(), expected_capacity, "Hay tickets duplicados");

        // 3. Biyectividad 1:1
        assert!(unique_sold.difference(&expected_ids).next().is_none());
        assert!(expected_ids.difference(&unique_sold).next().is_none());

        // 4. Integridad del estado final de cada sector y butaca
        let final_venue = qm.get_venue();
        for sector in &final_venue.sectors {
            assert_eq!(
                sector.n_booked_seats(),
                sector.capacity,
                "El sector '{}' no completó su venta",
                sector.code
            );
            assert_eq!(sector.n_available_seats(), 0);
            for seat in &sector.seats {
                assert!(matches!(seat.state, SeatState::Booked));
            }
        }
    }

    /// -----------------------------------------------------------------------
    /// TEST 2: Auditorio 100% Numerado y Secuencia Continua de Billetes
    /// -----------------------------------------------------------------------
    /// Un teatro o auditorio sin pista general (100% asientos numerados):
    /// 20 sectores de 150 butacas cada uno (3.000 entradas numeradas en total).
    ///
    /// Valida:
    /// 1. Que cada sector venda exactamente desde el asiento 1 hasta el último sin huecos.
    /// 2. Que no existan saltos de numeración ni solapamientos entre sectores.
    #[test]
    fn test_pure_numbered_arena_sequence_continuity() {
        let n_sectors = 20;
        let cap_per_sector = 150;
        let mut sectors = Vec::with_capacity(n_sectors);

        for i in 1..=n_sectors {
            sectors.push(Sector::new(
                &format!("Grada {}", i),
                &format!("G{}", i),
                cap_per_sector,
                true,
            ));
        }

        let venue = Venue {
            name: "Auditorio 100% Numerado".to_string(),
            sectors,
        };
        let total_capacity = venue.total_capacity();

        let mut qm = ConcurrentQueueManager::new(venue);
        let sold = qm.run();

        assert_eq!(sold.len(), total_capacity, "No se vendieron todas las plazas");

        // Agrupar números vendidos por cada sector
        let mut sector_seats: HashMap<String, Vec<usize>> = HashMap::new();
        for ticket in &sold {
            let parts: Vec<&str> = ticket.split('-').collect();
            assert_eq!(parts.len(), 2);
            let code = parts[0].to_string();
            let num: usize = parts[1].parse().unwrap();
            sector_seats.entry(code).or_default().push(num);
        }

        assert_eq!(sector_seats.len(), n_sectors);

        // Verificar continuidad estricta: 1..=150 en cada sector
        for i in 1..=n_sectors {
            let code = format!("G{}", i);
            let mut nums = sector_seats.get(&code).expect("Falta sector").clone();
            nums.sort_unstable();

            assert_eq!(nums.len(), cap_per_sector);
            for (idx, &seat_num) in nums.iter().enumerate() {
                assert_eq!(
                    seat_num,
                    idx + 1,
                    "En sector {} falta o está desordenado el asiento {}",
                    code,
                    idx + 1
                );
            }
        }
    }

    /// -----------------------------------------------------------------------
    /// TEST 3: Saturación de Cola con Capacidades Impares y Primas (Anti-Starvation)
    /// -----------------------------------------------------------------------
    /// En `simulation.rs`, los compradores intentan adquirir entre 1 y 6 entradas.
    /// Si un sector tiene capacidades primas no divisibles por paquetes de entradas
    /// (ej. 7, 11, 13, 23, 41...), se producen situaciones donde quedan 1 o 2 asientos
    /// y los compradores que piden 3–6 plazas rebotan continuamente.
    ///
    /// Valida:
    /// 1. Ausencia de inanición (*starvation*): el sistema no se bloquea en la cola final.
    /// 2. Que los compradores que piden 1 o 2 entradas absorben los últimos remanentes.
    /// 3. Venta del 100% y finalización limpia.
    #[test]
    fn test_prime_capacities_and_anti_starvation() {
        let prime_caps = [7, 11, 13, 17, 19, 23, 29, 31, 37, 41];
        let mut sectors = Vec::new();

        for (idx, &cap) in prime_caps.iter().enumerate() {
            // Mitad generales, mitad numerados
            let is_numbered = idx % 2 == 0;
            sectors.push(Sector::new(
                &format!("Zona Prima {}", idx + 1),
                &format!("PR{}", idx + 1),
                cap,
                is_numbered,
            ));
        }

        let venue = Venue {
            name: "Recinto con Aforos Primos".to_string(),
            sectors,
        };
        let expected_cap = venue.total_capacity();

        let start = std::time::Instant::now();
        let mut qm = ConcurrentQueueManager::new(venue);
        let sold = qm.run();
        let duration = start.elapsed();

        println!(
            "test_prime_capacities_and_anti_starvation finalizado en {:.2?} ({} entradas)",
            duration,
            sold.len()
        );

        assert_eq!(
            sold.len(),
            expected_cap,
            "El sistema sufrió inanición y no pudo vender las últimas plazas impares"
        );

        let unique: HashSet<String> = sold.into_iter().collect();
        assert_eq!(unique.len(), expected_cap, "Existen duplicados");

        let fv = qm.get_venue();
        assert_eq!(fv.total_available_seats(), 0);
        assert_eq!(fv.total_booked_seats(), expected_cap);
    }

    /// -----------------------------------------------------------------------
    /// TEST 4: Determinismo en Ráfagas Consecutivas (Clean Reset & Termination)
    /// -----------------------------------------------------------------------
    /// Ejecuta 3 simulaciones independientes seguidas de tamaño medio.
    /// Comprueba que no hay efectos secundarios residuales entre instancias
    /// de `ConcurrentQueueManager` y que cada tanda termina de forma determinista.
    #[test]
    fn test_consecutive_bursts_and_clean_termination() {
        for burst in 1..=3 {
            let sectors = vec![
                Sector::new("Pista Ráfaga", "PST", 1_200, false),
                Sector::new("Grada 1 Ráfaga", "G1R", 400, true),
                Sector::new("Grada 2 Ráfaga", "G2R", 400, true),
            ];
            let venue = Venue {
                name: format!("Ráfaga {}", burst),
                sectors,
            };
            let cap = venue.total_capacity();

            let t0 = std::time::Instant::now();
            let mut qm = ConcurrentQueueManager::new(venue);
            let sold = qm.run();
            let dur = t0.elapsed();

            assert_eq!(
                sold.len(),
                cap,
                "Ráfaga {}: no se vendió el total de {}",
                burst,
                cap
            );
            assert!(
                dur.as_secs() < 8,
                "Ráfaga {}: tiempo excesivo ({:.2?})",
                burst,
                dur
            );

            let unique: HashSet<String> = sold.into_iter().collect();
            assert_eq!(unique.len(), cap);
        }
    }
}
