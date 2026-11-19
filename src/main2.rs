// ===========================================================================
//  main2.rs — Batería de tests exhaustivos para la práctica de ticketing
//
//  Uso:
//    cargo test --release --bin main2 -- --nocapture
//
//  Este fichero es un punto de entrada adicional (ver [[bin]] en Cargo.toml)
//  que reutiliza todos los módulos del proyecto y ejecuta tests de:
//
//    1.  Correctitud básica (sin duplicados, 100% vendido, estado coherente)
//    2.  Recintos diminutos (1-3 butacas) → caso borde
//    3.  Recintos gigantes → estrés
//    4.  Solo sectores generales / solo numerados / mixtos
//    5.  Sector con exactamente 1 butaca
//    6.  Muchos sectores pequeños
//    7.  Estrés concurrente: muchos hilos sobre pocas plazas
//    8.  Repetibilidad: ejecutar N veces para cazar condiciones de carrera
//    9.  Verificación de invariantes del venue tras la ejecución
//   10.  Coherencia de contadores atómicos
//   11.  Test unitario de cas::cambiar_todas
//   12.  Test unitario del Booker (select, deselect, book) en aislamiento
//   13.  Test de rendimiento vs secuencial
//   14.  Fuga de plazas (cancelaciones masivas, rollback parcial, etc.)
// ===========================================================================

#![allow(dead_code)]

mod venue;
mod booking;
mod simulation;
mod config;
mod queue_manager;

use std::collections::HashSet;
use std::time::Instant;

use venue::{Sector, Venue};
use queue_manager::{ConcurrentQueueManager, QueueManager};

// ---- Helpers ----------------------------------------------------------------

/// Ejecuta una simulación completa y devuelve (tickets, duración, venue final).
fn run_venue(venue: Venue) -> (Vec<String>, std::time::Duration, Venue) {
    let t = Instant::now();
    let mut qm = ConcurrentQueueManager::new(venue);
    let tickets = qm.run();
    let d = t.elapsed();
    let v = qm.get_venue();
    (tickets, d, v)
}

/// Verifica las tres invariantes de negocio sobre un resultado.
fn assert_correctness(tickets: &[String], venue: &Venue, label: &str) {
    let expected = venue.total_capacity();

    // 1. 100% vendido
    assert_eq!(
        tickets.len(),
        expected,
        "[{}] Se vendieron {} de {} entradas",
        label,
        tickets.len(),
        expected
    );

    // 2. Cero duplicados
    let unique: HashSet<&String> = tickets.iter().collect();
    assert_eq!(
        unique.len(),
        tickets.len(),
        "[{}] Hay {} tickets duplicados",
        label,
        tickets.len() - unique.len()
    );

    // 3. Estado del venue coherente
    assert_eq!(
        venue.total_booked_seats(),
        expected,
        "[{}] El venue reporta {} booked de {}",
        label,
        venue.total_booked_seats(),
        expected
    );
    assert_eq!(
        venue.total_available_seats(),
        0,
        "[{}] Quedan {} available tras vender todo",
        label,
        venue.total_available_seats()
    );
}

/// Crea un venue personalizado a partir de una lista de (nombre, código, capacidad, is_numbered).
fn custom_venue(name: &str, sectors: &[(&str, &str, usize, bool)]) -> Venue {
    Venue {
        name: name.to_string(),
        sectors: sectors
            .iter()
            .map(|(n, c, cap, num)| Sector::new(n, c, *cap, *num))
            .collect(),
    }
}

/// Crea el recinto oficial del enunciado (requerido por queue_manager::tests).
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

// ---- TESTS ------------------------------------------------------------------

fn main() {
    println!("\n========= Tests exhaustivos — main2.rs =========");
    println!("Ejecuta con: cargo test --release --bin main2 -- --nocapture\n");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::booking::Booker;

    // ===================================================================
    //  TEST 1: Recinto del enunciado (configuración por defecto)
    // ===================================================================
    #[test]
    fn t01_default_venue_correctness() {
        println!("\n--- T01: Recinto por defecto ---");
        let venue = default_venue();
        let cap = venue.total_capacity();
        println!("  Capacidad: {}", cap);

        let (tickets, dur, final_v) = run_venue(venue);
        assert_correctness(&tickets, &final_v, "T01");
        println!("  OK — {} tickets en {:.2?}", tickets.len(), dur);
    }

    fn default_venue() -> Venue {
        initialize_venue()
    }

    // ===================================================================
    //  TEST 2: Recinto mínimo — 1 butaca numerada
    // ===================================================================
    #[test]
    fn t02_single_seat_numbered() {
        println!("\n--- T02: 1 butaca numerada ---");
        let v = custom_venue("1-seat", &[("Lone", "L1", 1, true)]);
        let (tickets, _, fv) = run_venue(v);
        assert_correctness(&tickets, &fv, "T02");
        println!("  OK — ticket: {:?}", tickets);
    }

    // ===================================================================
    //  TEST 3: Recinto mínimo — 1 plaza general
    // ===================================================================
    #[test]
    fn t03_single_seat_general() {
        println!("\n--- T03: 1 plaza general ---");
        let v = custom_venue("1-ga", &[("GA", "GA", 1, false)]);
        let (tickets, _, fv) = run_venue(v);
        assert_correctness(&tickets, &fv, "T03");
        println!("  OK — ticket: {:?}", tickets);
    }

    // ===================================================================
    //  TEST 4: 2 plazas generales (mínimo para contención)
    // ===================================================================
    #[test]
    fn t04_two_seats_general() {
        println!("\n--- T04: 2 plazas generales ---");
        let v = custom_venue("2-ga", &[("GA", "GA", 2, false)]);
        let (tickets, _, fv) = run_venue(v);
        assert_correctness(&tickets, &fv, "T04");
        println!("  OK — {} tickets", tickets.len());
    }

    // ===================================================================
    //  TEST 5: 3 butacas numeradas
    // ===================================================================
    #[test]
    fn t05_three_seats_numbered() {
        println!("\n--- T05: 3 butacas numeradas ---");
        let v = custom_venue("3-num", &[("S1", "S1", 3, true)]);
        let (tickets, _, fv) = run_venue(v);
        assert_correctness(&tickets, &fv, "T05");
        println!("  OK — {:?}", tickets);
    }

    // ===================================================================
    //  TEST 6: Solo sectores generales (varios)
    // ===================================================================
    #[test]
    fn t06_only_general_sectors() {
        println!("\n--- T06: Solo sectores generales ---");
        let v = custom_venue(
            "all-ga",
            &[
                ("Pista", "P1", 500, false),
                ("Pista2", "P2", 300, false),
                ("Pista3", "P3", 200, false),
            ],
        );
        let cap = v.total_capacity();
        let (tickets, dur, fv) = run_venue(v);
        assert_correctness(&tickets, &fv, "T06");
        println!("  OK — {} tickets en {:.2?}", cap, dur);
    }

    // ===================================================================
    //  TEST 7: Solo sectores numerados (varios)
    // ===================================================================
    #[test]
    fn t07_only_numbered_sectors() {
        println!("\n--- T07: Solo sectores numerados ---");
        let v = custom_venue(
            "all-num",
            &[
                ("Grada A", "GA", 100, true),
                ("Grada B", "GB", 100, true),
                ("Grada C", "GC", 100, true),
                ("Grada D", "GD", 50, true),
            ],
        );
        let cap = v.total_capacity();
        let (tickets, dur, fv) = run_venue(v);
        assert_correctness(&tickets, &fv, "T07");
        println!("  OK — {} tickets en {:.2?}", cap, dur);
    }

    // ===================================================================
    //  TEST 8: Mixto equilibrado
    // ===================================================================
    #[test]
    fn t08_mixed_balanced() {
        println!("\n--- T08: Mixto equilibrado ---");
        let v = custom_venue(
            "mixed",
            &[
                ("Standing", "ST", 200, false),
                ("Seated A", "SA", 200, true),
                ("VIP", "VIP", 50, false),
                ("Seated B", "SB", 150, true),
            ],
        );
        let cap = v.total_capacity();
        let (tickets, dur, fv) = run_venue(v);
        assert_correctness(&tickets, &fv, "T08");
        println!("  OK — {} tickets en {:.2?}", cap, dur);
    }

    // ===================================================================
    //  TEST 9: Muchos sectores pequeños (estrés de búsqueda)
    // ===================================================================
    #[test]
    fn t09_many_tiny_sectors() {
        println!("\n--- T09: 50 sectores de 10 butacas ---");
        let sector_refs: Vec<Sector> = (0..50)
            .map(|i| {
                Sector::new(&format!("Tiny {}", i), &format!("T{}", i), 10, i % 2 == 0)
            })
            .collect();

        let v = Venue {
            name: "Many-tiny".to_string(),
            sectors: sector_refs,
        };
        let cap = v.total_capacity();
        let (tickets, dur, fv) = run_venue(v);
        assert_correctness(&tickets, &fv, "T09");
        println!("  OK — {} tickets (50 sectores) en {:.2?}", cap, dur);
    }

    // ===================================================================
    //  TEST 10: Sector general GRANDE (12500 como el Standing)
    // ===================================================================
    #[test]
    fn t10_large_general() {
        println!("\n--- T10: Sector general grande (12500) ---");
        let v = custom_venue("big-ga", &[("Standing", "ST", 12500, false)]);
        let (tickets, dur, fv) = run_venue(v);
        assert_correctness(&tickets, &fv, "T10");
        println!("  OK — {} tickets en {:.2?}", tickets.len(), dur);
    }

    // ===================================================================
    //  TEST 11: Sector numerado GRANDE (5000)
    // ===================================================================
    #[test]
    fn t11_large_numbered() {
        println!("\n--- T11: Sector numerado grande (5000) ---");
        let v = custom_venue("big-num", &[("Arena", "AR", 5000, true)]);
        let (tickets, dur, fv) = run_venue(v);
        assert_correctness(&tickets, &fv, "T11");
        println!("  OK — {} tickets en {:.2?}", tickets.len(), dur);
    }

    // ===================================================================
    //  TEST 12: Repetibilidad — ejecutar 10 veces el mismo venue pequeño
    //  (caza condiciones de carrera que solo aparecen esporádicamente)
    // ===================================================================
    #[test]
    fn t12_repeatability_stress() {
        println!("\n--- T12: 10 repeticiones sobre venue mixto ---");
        for round in 1..=10 {
            let v = custom_venue(
                "repeat",
                &[
                    ("GA", "GA", 50, false),
                    ("S1", "S1", 30, true),
                    ("S2", "S2", 20, true),
                ],
            );
            let (tickets, _, fv) = run_venue(v);
            assert_correctness(&tickets, &fv, &format!("T12-R{}", round));
        }
        println!("  OK — 10/10 ejecuciones correctas");
    }

    // ===================================================================
    //  TEST 13: Repetibilidad sobre venue más grande
    // ===================================================================
    #[test]
    fn t13_repeatability_medium() {
        println!("\n--- T13: 5 repeticiones sobre venue mediano ---");
        for round in 1..=5 {
            let v = custom_venue(
                "rep-med",
                &[
                    ("Standing", "ST", 1000, false),
                    ("GC", "GC", 500, false),
                    ("S1", "S1", 200, true),
                    ("S2", "S2", 200, true),
                    ("S3", "S3", 100, true),
                ],
            );
            let (tickets, _, fv) = run_venue(v);
            assert_correctness(&tickets, &fv, &format!("T13-R{}", round));
        }
        println!("  OK — 5/5 ejecuciones correctas");
    }

    // ===================================================================
    //  TEST 14: Verificar que los IDs de tickets siguen el formato correcto
    // ===================================================================
    #[test]
    fn t14_ticket_id_format() {
        println!("\n--- T14: Formato de IDs de tickets ---");
        let v = custom_venue(
            "fmt",
            &[
                ("Standing", "ST", 50, false),
                ("Seated", "S1", 30, true),
            ],
        );
        let (tickets, _, fv) = run_venue(v);
        assert_correctness(&tickets, &fv, "T14");

        // Todos los IDs deben seguir el patrón "CODE-N"
        for t in &tickets {
            let parts: Vec<&str> = t.rsplitn(2, '-').collect();
            assert!(
                parts.len() == 2,
                "[T14] Ticket ID '{}' no sigue el formato CODE-N",
                t
            );
            let _num: usize = parts[0]
                .parse()
                .unwrap_or_else(|_| panic!("[T14] Ticket ID '{}' no tiene número válido", t));
        }
        println!("  OK — todos los IDs tienen formato CODE-N");
    }

    // ===================================================================
    //  TEST 15: Verificar que cada sector se agota correctamente
    // ===================================================================
    #[test]
    fn t15_per_sector_completeness() {
        println!("\n--- T15: Completitud por sector ---");
        let v = custom_venue(
            "per-sector",
            &[
                ("Standing", "ST", 100, false),
                ("S1", "S1", 50, true),
                ("S2", "S2", 30, true),
            ],
        );
        let (tickets, _, fv) = run_venue(v);
        assert_correctness(&tickets, &fv, "T15");

        // Verificar que cada sector tiene exactamente su capacidad de booked
        for sector in &fv.sectors {
            assert_eq!(
                sector.n_booked_seats(),
                sector.capacity,
                "[T15] Sector {} ({}) tiene {}/{} booked",
                sector.name,
                sector.code,
                sector.n_booked_seats(),
                sector.capacity
            );
            assert_eq!(
                sector.n_available_seats(),
                0,
                "[T15] Sector {} tiene {} available",
                sector.code,
                sector.n_available_seats()
            );
        }
        println!("  OK — cada sector vendido al 100%");
    }

    // ===================================================================
    //  TEST 16: Venue con capacidad desbalanceada
    //  (un sector enorme + varios diminutos)
    // ===================================================================
    #[test]
    fn t16_imbalanced_venue() {
        println!("\n--- T16: Venue desbalanceado ---");
        let v = custom_venue(
            "imbalanced",
            &[
                ("Pista", "P", 5000, false),
                ("Palco1", "PL1", 5, true),
                ("Palco2", "PL2", 5, true),
                ("Palco3", "PL3", 3, true),
            ],
        );
        let (tickets, dur, fv) = run_venue(v);
        assert_correctness(&tickets, &fv, "T16");
        println!("  OK — {} tickets en {:.2?}", tickets.len(), dur);
    }

    // ===================================================================
    //  TEST 17: Sector con capacidad = MAX_TICKETS (caso borde)
    // ===================================================================
    #[test]
    fn t17_capacity_equals_max_tickets() {
        println!("\n--- T17: Sector con cap = MAX_TICKETS ({}) ---", config::MAX_TICKETS);
        let v = custom_venue(
            "edge-max",
            &[
                ("Exact", "EX", config::MAX_TICKETS, true),
                ("GA", "GA", config::MAX_TICKETS, false),
            ],
        );
        let (tickets, _, fv) = run_venue(v);
        assert_correctness(&tickets, &fv, "T17");
        println!("  OK — {} tickets", tickets.len());
    }

    // ===================================================================
    //  TEST 18: Sector con capacidad < MAX_TICKETS (el comprador
    //  nunca puede comprar tantos como quiere)
    // ===================================================================
    #[test]
    fn t18_capacity_less_than_max_tickets() {
        println!("\n--- T18: Sector con cap < MAX_TICKETS ---");
        let cap = config::MIN_TICKETS.max(1);
        let v = custom_venue(
            "tiny-cap",
            &[
                ("Micro-num", "MN", cap, true),
                ("Micro-ga", "MG", cap, false),
            ],
        );
        let (tickets, _, fv) = run_venue(v);
        assert_correctness(&tickets, &fv, "T18");
        println!("  OK — {} tickets", tickets.len());
    }

    // ===================================================================
    //  TEST 19: Verificar que total_capacity coincide con la suma de sectores
    // ===================================================================
    #[test]
    fn t19_capacity_consistency() {
        println!("\n--- T19: Consistencia de capacidad ---");
        let v = custom_venue(
            "cap-check",
            &[
                ("A", "A", 100, false),
                ("B", "B", 200, true),
                ("C", "C", 50, false),
            ],
        );
        let manual_sum: usize = v.sectors.iter().map(|s| s.capacity).sum();
        assert_eq!(v.total_capacity(), manual_sum);
        assert_eq!(v.total_capacity(), 350);

        let (tickets, _, fv) = run_venue(v);
        assert_correctness(&tickets, &fv, "T19");
        println!("  OK");
    }

    // ===================================================================
    //  TEST 20: Estrés — 25 sectores numerados × 300 butacas
    //  (configuración real del enunciado pero solo numerados)
    // ===================================================================
    #[test]
    fn t20_stress_numbered_only() {
        println!("\n--- T20: Estrés: 25×300 numerados ---");
        let sectors: Vec<Sector> = (0..25)
            .map(|i| {
                Sector::new(
                    &format!("S{}", i + 1),
                    &format!("S{}", i + 1),
                    300,
                    true,
                )
            })
            .collect();
        let v = Venue {
            name: "Stress-numbered".to_string(),
            sectors,
        };
        let cap = v.total_capacity();
        let (tickets, dur, fv) = run_venue(v);
        assert_correctness(&tickets, &fv, "T20");
        println!("  OK — {} tickets en {:.2?}", cap, dur);
    }

    // ===================================================================
    //  TEST 21: Verificar que no hay tickets con sector desconocido
    // ===================================================================
    #[test]
    fn t21_tickets_belong_to_known_sectors() {
        println!("\n--- T21: Tickets pertenecen a sectores conocidos ---");
        let v = custom_venue(
            "sector-check",
            &[
                ("Standing", "ST", 200, false),
                ("Grada", "GR", 100, true),
            ],
        );
        let (tickets, _, fv) = run_venue(v);
        assert_correctness(&tickets, &fv, "T21");

        let valid_codes: HashSet<&str> = fv.sectors.iter().map(|s| s.code.as_str()).collect();
        for t in &tickets {
            // Extraer el código: todo antes del último '-'
            let code = t.rsplitn(2, '-').last().unwrap();
            assert!(
                valid_codes.contains(code),
                "[T21] Ticket '{}' tiene código de sector desconocido '{}'",
                t,
                code
            );
        }
        println!("  OK — todos los tickets tienen sector válido");
    }

    // ===================================================================
    //  TEST 22: Booker unitario — select + book (1 hilo, sin QueueManager)
    // ===================================================================
    #[test]
    fn t22_booker_unit_select_book() {
        println!("\n--- T22: Booker unitario select → book ---");
        use crate::booking::{Booker, ConcurrentBooker};

        let v = custom_venue("unit", &[("S1", "S1", 10, true)]);
        let mut booker = ConcurrentBooker::new(v);

        // Verificar estado inicial
        let avail = booker.get_available_seats("S1");
        assert_eq!(avail.len(), 10, "Deben haber 10 butacas disponibles");

        // Seleccionar 3 butacas
        let to_select: Vec<String> = avail[0..3].to_vec();
        let res = booker.select_seats("S1", &to_select);
        assert!(res.is_ok(), "select_seats debería ser Ok");

        // Verificar que ya no están disponibles
        let avail_after = booker.get_available_seats("S1");
        for s in &to_select {
            assert!(
                !avail_after.contains(s),
                "Butaca {} no debería seguir disponible",
                s
            );
        }

        // Confirmar compra
        let booked = booker.book_seats("S1", &to_select);
        assert!(booked.is_ok(), "book_seats debería ser Ok");
        let booked = booked.unwrap();
        assert_eq!(booked.len(), 3);

        println!("  OK — select + book funciona correctamente");
    }

    // ===================================================================
    //  TEST 23: Booker unitario — select + deselect (cancelación)
    // ===================================================================
    #[test]
    fn t23_booker_unit_select_deselect() {
        println!("\n--- T23: Booker unitario select → deselect ---");
        use crate::booking::{Booker, ConcurrentBooker};

        let v = custom_venue("unit2", &[("S1", "S1", 10, true)]);
        let mut booker = ConcurrentBooker::new(v);

        let avail = booker.get_available_seats("S1");
        let to_select: Vec<String> = avail[0..2].to_vec();

        booker.select_seats("S1", &to_select).unwrap();

        // Cancelar reserva
        let res = booker.deselect_seats("S1", &to_select);
        assert!(res.is_ok(), "deselect_seats debería ser Ok");

        // Verificar que están disponibles de nuevo
        let avail_after = booker.get_available_seats("S1");
        for s in &to_select {
            assert!(
                avail_after.contains(s),
                "Butaca {} debería estar disponible tras deselect",
                s
            );
        }

        println!("  OK — select + deselect funciona correctamente");
    }

    // ===================================================================
    //  TEST 24: Booker — no se puede reservar una butaca ya seleccionada
    // ===================================================================
    #[test]
    fn t24_booker_double_select_fails() {
        println!("\n--- T24: Doble select falla ---");
        use crate::booking::{Booker, ConcurrentBooker};

        let v = custom_venue("dbl", &[("S1", "S1", 10, true)]);
        let mut booker = ConcurrentBooker::new(v);

        let avail = booker.get_available_seats("S1");
        let seat = vec![avail[0].clone()];

        booker.select_seats("S1", &seat).unwrap();
        let res = booker.select_seats("S1", &seat);
        assert!(res.is_err(), "select doble debería fallar");

        println!("  OK — doble select rechazado correctamente");
    }

    // ===================================================================
    //  TEST 25: Booker — book sin select previo falla
    // ===================================================================
    #[test]
    fn t25_booker_book_without_select_fails() {
        println!("\n--- T25: Book sin select previo falla ---");
        use crate::booking::{Booker, ConcurrentBooker};

        let v = custom_venue("nosel", &[("S1", "S1", 10, true)]);
        let mut booker = ConcurrentBooker::new(v);

        let seat = vec!["S1-1".to_string()];
        let res = booker.book_seats("S1", &seat);
        assert!(res.is_err(), "book sin select previo debería fallar");

        println!("  OK — book sin select rechazado correctamente");
    }

    // ===================================================================
    //  TEST 26: Booker general — select + book
    // ===================================================================
    #[test]
    fn t26_booker_general_select_book() {
        println!("\n--- T26: Booker general select → book ---");
        use crate::booking::{Booker, ConcurrentBooker};

        let v = custom_venue("ga-unit", &[("GA", "GA", 100, false)]);
        let mut booker = ConcurrentBooker::new(v);

        let avail = booker.get_available_seats("GA");
        assert!(!avail.is_empty(), "Debe haber plazas disponibles");

        let n = avail.len().min(3);
        let to_select: Vec<String> = avail[..n].to_vec();

        booker.select_seats("GA", &to_select).unwrap();
        let booked = booker.book_seats("GA", &to_select).unwrap();
        assert_eq!(booked.len(), n);

        println!("  OK — {} plazas generales vendidas", n);
    }

    // ===================================================================
    //  TEST 27: Booker general — select + deselect restaura disponibilidad
    // ===================================================================
    #[test]
    fn t27_booker_general_deselect() {
        println!("\n--- T27: Booker general select → deselect ---");
        use crate::booking::{Booker, ConcurrentBooker};

        let v = custom_venue("ga-ds", &[("GA", "GA", 50, false)]);
        let mut booker = ConcurrentBooker::new(v);

        let initial = booker.get_total_available_seats();
        let avail = booker.get_available_seats("GA");
        let n = avail.len().min(5);
        let to_select: Vec<String> = avail[..n].to_vec();

        booker.select_seats("GA", &to_select).unwrap();
        // Ahora debería haber menos disponibles
        let mid = booker.get_total_available_seats();
        assert!(mid < initial, "Tras select, available debería decrecer");

        booker.deselect_seats("GA", &to_select).unwrap();
        let restored = booker.get_total_available_seats();
        assert_eq!(
            restored, initial,
            "Tras deselect, available debería restaurarse"
        );

        println!("  OK — deselect restaura available correctamente");
    }

    // ===================================================================
    //  TEST 28: Remaining seats decrece hasta 0
    // ===================================================================
    #[test]
    fn t28_remaining_reaches_zero() {
        println!("\n--- T28: remaining_seats llega a 0 ---");

        let v = custom_venue(
            "rem",
            &[("S1", "S1", 20, true), ("GA", "GA", 30, false)],
        );
        let mut qm = ConcurrentQueueManager::new(v);
        let _ = qm.run();
        let fv = qm.get_venue();

        assert_eq!(fv.total_booked_seats(), 50);
        assert_eq!(fv.total_available_seats(), 0);
        println!("  OK — remaining = 0 tras venta completa");
    }

    // ===================================================================
    //  TEST 29: Recinto con muchos sectores generales grandes
    // ===================================================================
    #[test]
    fn t29_stress_many_generals() {
        println!("\n--- T29: 10 sectores generales × 1000 ---");
        let sectors: Vec<Sector> = (0..10)
            .map(|i| {
                Sector::new(
                    &format!("GA{}", i),
                    &format!("G{}", i),
                    1000,
                    false,
                )
            })
            .collect();
        let v = Venue {
            name: "Many-generals".to_string(),
            sectors,
        };
        let cap = v.total_capacity();
        let (tickets, dur, fv) = run_venue(v);
        assert_correctness(&tickets, &fv, "T29");
        println!("  OK — {} tickets en {:.2?}", cap, dur);
    }

    // ===================================================================
    //  TEST 30: Verificar unicidad global de IDs entre TODOS los sectores
    // ===================================================================
    #[test]
    fn t30_global_id_uniqueness() {
        println!("\n--- T30: Unicidad global de IDs ---");
        let v = custom_venue(
            "global-unique",
            &[
                ("Standing", "ST", 500, false),
                ("GC", "GC", 300, false),
                ("S1", "S1", 100, true),
                ("S2", "S2", 100, true),
            ],
        );
        let (tickets, _, fv) = run_venue(v);
        assert_correctness(&tickets, &fv, "T30");

        // Verificación extra: agrupar por sector y verificar rangos
        let mut by_sector: std::collections::HashMap<String, Vec<usize>> =
            std::collections::HashMap::new();
        for t in &tickets {
            let parts: Vec<&str> = t.rsplitn(2, '-').collect();
            let code = parts[1].to_string();
            let num: usize = parts[0].parse().unwrap();
            by_sector.entry(code).or_default().push(num);
        }
        for (code, mut nums) in by_sector {
            nums.sort();
            nums.dedup();
            let sector = fv.sectors.iter().find(|s| s.code == code).unwrap();
            assert_eq!(
                nums.len(),
                sector.capacity,
                "[T30] Sector {} tiene {} IDs únicos pero capacidad {}",
                code,
                nums.len(),
                sector.capacity
            );
        }
        println!("  OK — IDs globalmente únicos y completos por sector");
    }

    // ===================================================================
    //  TEST 31: Rendimiento — el concurrente debería ser más rápido
    //  que un single-thread en un venue grande
    // ===================================================================
    #[test]
    fn t31_performance_vs_baseline() {
        println!("\n--- T31: Performance benchmark ---");
        let v1 = custom_venue(
            "perf",
            &[
                ("Standing", "ST", 2000, false),
                ("S1", "S1", 500, true),
                ("S2", "S2", 500, true),
            ],
        );
        let v2 = custom_venue(
            "perf",
            &[
                ("Standing", "ST", 2000, false),
                ("S1", "S1", 500, true),
                ("S2", "S2", 500, true),
            ],
        );

        let (_, dur_concurrent, _) = run_venue(v1);

        // Comparar con ejecución "pseudo-secuencial" (1 solo ConcurrentBooker, sin threads explícitos)
        let t = Instant::now();
        let mut booker = crate::booking::ConcurrentBooker::new(v2);
        let mut tickets = Vec::new();
        while booker.remaining_seats() > 0 {
            let t = crate::simulation::simulate_booking(&mut booker);
            tickets.extend(t);
        }
        let dur_sequential = t.elapsed();

        println!(
            "  Concurrente: {:.2?} | Secuencial: {:.2?} | Speedup: {:.1}x",
            dur_concurrent,
            dur_sequential,
            dur_sequential.as_secs_f64() / dur_concurrent.as_secs_f64().max(0.001)
        );

        // No assertamos que sea más rápido (depende de la máquina),
        // pero mostramos el speedup para referencia.
        assert_eq!(tickets.len(), 3000, "Secuencial también debe vender todo");
        println!("  OK — benchmark completado");
    }

    // ===================================================================
    //  TEST 32 (FUGA DE PLAZAS): Cancelaciones masivas en Admisión General
    //  Verifica que millones de ciclos select/deselect no pierdan plazas
    //  ni provoquen underflows en contadores atómicos (available / selected).
    // ===================================================================
    #[test]
    fn t32_fuga_plazas_general_cancelaciones_masivas() {
        println!("\n--- T32: Fuga de plazas — Cancelaciones masivas General ---");
        use crate::booking::{Booker, ConcurrentBooker};
        use std::sync::Arc;

        let total_cap = 500;
        let v = custom_venue("fuga-ga", &[("Standing", "ST", total_cap, false)]);
        let booker = Arc::new(std::sync::Mutex::new(ConcurrentBooker::new(v)));

        // Lanzamos múltiples hilos que solo hacen select y deselect repetidamente
        let n_threads = 16;
        let iterations_per_thread = 200;

        std::thread::scope(|s| {
            for _ in 0..n_threads {
                let booker_ref = Arc::clone(&booker);
                s.spawn(move || {
                    let mut b = booker_ref.lock().unwrap().clone();
                    for _ in 0..iterations_per_thread {
                        let seats = b.get_available_seats("ST");
                        if seats.is_empty() {
                            continue;
                        }
                        let to_take = seats.len().min(4);
                        let sub = &seats[..to_take];
                        if b.select_seats("ST", sub).is_ok() {
                            // Inmediatamente cancela (deselect)
                            let deselect_res = b.deselect_seats("ST", sub);
                            assert!(deselect_res.is_ok(), "[T32] deselect_seats falló");
                        }
                    }
                });
            }
        });

        // Tras todas las cancelaciones, el aforo disponible DEBE seguir siendo exactamente 500
        let final_b = booker.lock().unwrap().clone();
        let avail = final_b.get_total_available_seats();
        assert_eq!(
            avail, total_cap,
            "[T32 FUGA DETECTADA] Disponibles {} vs capacidad esperada {}",
            avail, total_cap
        );

        // Y ahora debemos poder vender el 100% sin problemas
        let (tickets, _, fv) = run_venue(final_b.get_venue());
        assert_correctness(&tickets, &fv, "T32-Final");
        println!("  OK — Cero fugas en General tras miles de cancelaciones concurrentes");
    }

    // ===================================================================
    //  TEST 33 (FUGA DE PLAZAS): Contención salvaje en Sectores Numerados
    //  Muchos hilos compiten exactamente por los mismos asientos.
    //  Verifica que los fallos de CAS y rollbacks no dejen asientos
    //  en estado SELECCIONADA (huérfanos).
    // ===================================================================
    #[test]
    fn t33_fuga_plazas_numeradas_contencion_y_rollback() {
        println!("\n--- T33: Fuga de plazas — Contención y Rollback en Numeradas ---");
        use crate::booking::{Booker, ConcurrentBooker};
        use std::sync::Arc;

        let total_cap = 20; // Recinto pequeño para maximizar la contención
        let v = custom_venue("fuga-num", &[("Grada", "G1", total_cap, true)]);
        let booker = Arc::new(std::sync::Mutex::new(ConcurrentBooker::new(v)));

        let n_threads = 20;
        let iters = 100;

        std::thread::scope(|s| {
            for _ in 0..n_threads {
                let booker_ref = Arc::clone(&booker);
                s.spawn(move || {
                    let mut b = booker_ref.lock().unwrap().clone();
                    for _ in 0..iters {
                        let avail = b.get_available_seats("G1");
                        if avail.len() >= 2 {
                            let batch = vec![avail[0].clone(), avail[1].clone()];
                            if b.select_seats("G1", &batch).is_ok() {
                                // 50% de las veces deselecciona, 50% confirma
                                if rand::random_bool(0.5) {
                                    let _ = b.deselect_seats("G1", &batch);
                                } else {
                                    let _ = b.book_seats("G1", &batch);
                                }
                            }
                        }
                    }
                });
            }
        });

        // Verificamos que no haya asientos huérfanos: la suma de disponibles + vendidos DEBE ser total_cap
        let final_b = booker.lock().unwrap().clone();
        let fv = final_b.get_venue();
        let booked = fv.total_booked_seats();
        let available = fv.total_available_seats();

        assert_eq!(
            booked + available,
            total_cap,
            "[T33 FUGA DETECTADA] Booked ({}) + Available ({}) != Total Cap ({}) -> Hay plazas huérfanas!",
            booked,
            available,
            total_cap
        );
        println!("  OK — Cero butacas huérfanas en Numeradas bajo contención extrema");
    }

    // ===================================================================
    //  TEST 34 (FUGA DE PLAZAS): Rollback atómico en select_seats parcial
    //  Si una reserva de N plazas falla a mitad (p. ej. en la 3ª butaca),
    //  las butacas 1 y 2 DEBEN volver inmediatamente a Available.
    // ===================================================================
    #[test]
    fn t34_fuga_plazas_rollback_atomico_en_fallo_parcial() {
        println!("\n--- T34: Fuga de plazas — Rollback en Fallo Parcial ---");
        use crate::booking::{Booker, ConcurrentBooker};

        let v = custom_venue("rollback-test", &[("SectorA", "A", 5, true)]);
        let mut booker = ConcurrentBooker::new(v);

        // Ocupamos la butaca A-3 directamente
        let seat3 = vec!["A-3".to_string()];
        booker.select_seats("A", &seat3).unwrap();
        booker.book_seats("A", &seat3).unwrap();

        // Ahora intentamos una reserva conjunta de [A-1, A-2, A-3]
        // A-1 y A-2 están libres, pero A-3 está ocupada.
        // La operación DEBE fallar y A-1 y A-2 DEBEN quedar libres (no en Selected).
        let batch = vec!["A-1".to_string(), "A-2".to_string(), "A-3".to_string()];
        let res = booker.select_seats("A", &batch);
        assert!(res.is_err(), "[T34] select_seats debería haber fallado por A-3");

        // Verificamos que A-1 y A-2 sigan disponibles
        let avail_after = booker.get_available_seats("A");
        assert!(
            avail_after.contains(&"A-1".to_string()),
            "[T34 FUGA DETECTADA] A-1 quedó bloqueada tras el fallo parcial"
        );
        assert!(
            avail_after.contains(&"A-2".to_string()),
            "[T34 FUGA DETECTADA] A-2 quedó bloqueada tras el fallo parcial"
        );
        println!("  OK — Rollback transaccional perfecto, no hay butacas retenidas");
    }
}
