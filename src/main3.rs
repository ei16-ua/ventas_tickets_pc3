#![allow(dead_code)]

mod venue;
mod booking;
mod simulation;
mod config;
mod queue_manager;

use std::collections::HashSet;
use venue::{Sector, Venue};
use queue_manager::{ConcurrentQueueManager, QueueManager};

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
    println!("\n========= Tests de Escenario Real — main3.rs =========");
    println!("Ejecuta con: cargo test --release --bin main3 -- --nocapture\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test exhaustivo sobre el escenario real de producción (`initialize_venue()`).
    ///
    /// Valida minuciosamente todos los requisitos de `enunciado.md`:
    /// 1. Venta completa: 100% de aforo vendido (exactamente 22.500 localidades).
    /// 2. Cero duplicados: todos los identificadores son únicos.
    /// 3. Correspondencia biyectiva 1:1 con el inventario del recinto.
    /// 4. Invariante sectorial: cada uno de los 27 sectores vende el 100% de su capacidad.
    /// 5. Ausencia total de reservas huérfanas: ningún asiento en estado `Selected`.
    /// 6. Estado final del `Venue` completamente coherente.
    #[test]
    fn test_release_scenario_full_exhaustiveness() {
        let venue = crate::initialize_venue();
        let expected_capacity = venue.total_capacity();
        assert_eq!(
            expected_capacity, 22_500,
            "El recinto real de release debe tener exactamente 22.500 localidades"
        );

        // Construir el conjunto de todas las localidades legítimas del recinto inicial
        let mut expected_seat_ids: HashSet<String> = HashSet::with_capacity(expected_capacity);
        for sector in &venue.sectors {
            for seat in &sector.seats {
                expected_seat_ids.insert(seat.id.clone());
            }
        }
        assert_eq!(expected_seat_ids.len(), expected_capacity);

        // Ejecutar el gestor de colas concurrente
        let time_start = std::time::Instant::now();
        let mut qm = ConcurrentQueueManager::new(venue);
        let sold_tickets = qm.run();
        let elapsed = time_start.elapsed();

        println!(
            "test_release_scenario_full_exhaustiveness finalizado en {:.2?} con {} tickets",
            elapsed,
            sold_tickets.len()
        );

        // 1. Venta completa: longitud exacta
        assert_eq!(
            sold_tickets.len(),
            expected_capacity,
            "Se debieron vender exactamente {} entradas, pero se vendieron {}",
            expected_capacity,
            sold_tickets.len()
        );

        // 2. Cero duplicados
        let unique_sold: HashSet<String> = sold_tickets.iter().cloned().collect();
        assert_eq!(
            unique_sold.len(),
            expected_capacity,
            "Se detectaron {} duplicados en la venta",
            sold_tickets.len() - unique_sold.len()
        );

        // 3. Correspondencia biyectiva: cada ticket vendido debe pertenecer al recinto legítimo
        let difference: Vec<&String> = unique_sold.difference(&expected_seat_ids).collect();
        assert!(
            difference.is_empty(),
            "Se encontraron tickets vendidos que no existen en el recinto: {:?}",
            difference
        );

        let missing: Vec<&String> = expected_seat_ids.difference(&unique_sold).collect();
        assert!(
            missing.is_empty(),
            "Se encontraron asientos del recinto que no fueron vendidos: {:?}",
            missing
        );

        // 4. Verificación sectorial y ausencia de reservas huérfanas
        let final_venue = qm.get_venue();
        for sector in &final_venue.sectors {
            assert_eq!(
                sector.n_booked_seats(),
                sector.capacity,
                "El sector '{}' ({}) no vendió todo su aforo: {}/{}",
                sector.name,
                sector.code,
                sector.n_booked_seats(),
                sector.capacity
            );
            assert_eq!(
                sector.n_available_seats(),
                0,
                "El sector '{}' ({}) aún tiene asientos Available: {}",
                sector.name,
                sector.code,
                sector.n_available_seats()
            );

            // Verificar asiento por asiento que cada uno está en estado Booked
            for seat in &sector.seats {
                assert!(
                    matches!(seat.state, crate::venue::SeatState::Booked),
                    "El asiento '{}' del sector '{}' quedó en estado no-Booked (reserva huérfana)",
                    seat.id,
                    sector.code
                );
            }
        }

        // 5. Estado global del Venue
        assert_eq!(final_venue.total_booked_seats(), expected_capacity);
        assert_eq!(final_venue.total_available_seats(), 0);
    }

    /// Test que comprueba el desglose por sector de los tickets vendidos en el escenario real.
    ///
    /// Valida que el conteo de billetes vendidos para cada código de sector (ST, GC, S1..S25)
    /// sea exactamente idéntico a su capacidad configurada en `config.rs`.
    #[test]
    fn test_release_scenario_sector_breakdown_and_ticket_formats() {
        let venue = crate::initialize_venue();
        let mut expected_sector_capacities: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for s in &venue.sectors {
            expected_sector_capacities.insert(s.code.clone(), s.capacity);
        }

        let mut qm = ConcurrentQueueManager::new(venue);
        let sold_tickets = qm.run();

        // Agrupar los tickets vendidos por el prefijo de sector
        let mut actual_sector_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();

        for ticket in &sold_tickets {
            // Formato esperado: "{code}-{number}"
            let parts: Vec<&str> = ticket.split('-').collect();
            assert_eq!(
                parts.len(),
                2,
                "El identificador de ticket '{}' no sigue el formato 'SECTOR-NUM'",
                ticket
            );

            let sector_code = parts[0].to_string();
            let seat_num: usize = parts[1].parse().unwrap_or_else(|_| {
                panic!("El número de asiento en '{}' no es un entero válido", ticket)
            });

            let cap = *expected_sector_capacities
                .get(&sector_code)
                .unwrap_or_else(|| panic!("Sector desconocido '{}' en ticket {}", sector_code, ticket));

            assert!(
                seat_num >= 1 && seat_num <= cap,
                "El número de asiento {} en '{}' está fuera del rango [1, {}]",
                seat_num,
                ticket,
                cap
            );

            *actual_sector_counts.entry(sector_code).or_insert(0) += 1;
        }

        // Comparar conteo sector por sector
        for (code, expected_cap) in expected_sector_capacities {
            let actual_count = actual_sector_counts.get(&code).copied().unwrap_or(0);
            assert_eq!(
                actual_count, expected_cap,
                "Discrepancia en sector '{}': esperados {} tickets, obtenidos {}",
                code, expected_cap, actual_count
            );
        }
    }

    /// Test de estrés y liveness: asegura que múltiples ejecuciones del escenario real
    /// no sufren interbloqueos, carreras ni pérdidas de vivacidad.
    #[test]
    fn test_release_scenario_liveness_and_termination() {
        // Ejecutar 2 veces consecutivas el escenario completo de release
        for round in 1..=2 {
            let venue = crate::initialize_venue();
            let capacity = venue.total_capacity();
            let mut qm = ConcurrentQueueManager::new(venue);

            let start = std::time::Instant::now();
            let sold = qm.run();
            let elapsed = start.elapsed();

            assert_eq!(
                sold.len(),
                capacity,
                "Ronda {}: no se vendieron todas las entradas",
                round
            );

            // Debe finalizar en un tiempo acotado (máximo 15s para 22.500 tickets en multihilo)
            assert!(
                elapsed.as_secs() < 15,
                "Ronda {}: tiempo excesivo ({:.2?}), posible contención o bloqueo",
                round,
                elapsed
            );
        }
    }
}
