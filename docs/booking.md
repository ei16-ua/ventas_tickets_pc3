# `booking.rs`: motor de reservas

## Papel del fichero

Este módulo define la API del backend (`Booker`) y contiene dos implementaciones:

- `SequentialBooker`: referencia secuencial, útil para comparar el contrato.
- `ConcurrentBooker`: motor compartido por los hilos de compra.

La simulación solo conoce el trait `Booker`; por tanto, no depende de cómo se sincroniza el estado interno.

## Contrato de una compra

Una sesión sigue esta secuencia:

```text
LIBRE --select_seats--> SELECCIONADA --book_seats--> VENDIDA
                                 |
                                 +--deselect_seats--> LIBRE
```

`select_seats`, `deselect_seats` y `book_seats` devuelven error si la transición no es válida. Una venta satisfactoria devuelve sus identificadores definitivos.

## Diseño concurrente actual

`ConcurrentBooker` posee un `Arc<ConcurrentVenue>`. Cada clon compartido por un worker opera sobre el mismo recinto, sin bloquear el recinto completo.

- **Sectores numerados:** cada butaca usa un `AtomicU8` con los estados `LIBRE`, `SELECCIONADA` y `VENDIDA`. Las transiciones de varias butacas se hacen mediante CAS y rollback seguro en `cas.rs`.
- **Admisión general:** mantiene contadores atómicos de plazas libres, seleccionadas y vendidas. `next_ticket` asigna los identificadores de las entradas emitidas.
- **Total vendido:** `ConcurrentVenue::total_booked` es el contador global que usa el gestor para decidir cuándo ha finalizado la venta.

Las cargas y actualizaciones usan `Acquire`, `Release` o `AcqRel`. La intención es que, una vez publicada una transición de estado, los demás hilos no observen datos anteriores de esa operación.

## Invariantes que debe mantener

1. Una butaca numerada nunca puede estar en dos estados a la vez.
2. Una butaca solo puede pasar a `VENDIDA` si estaba seleccionada.
3. El número total de tickets vendidos no supera la capacidad.
4. Cada ID retornado por `book_seats` aparece una única vez.
5. Una operación que falla no debe dejar una reserva parcial.

Son propiedades de **seguridad**: evitan ventas duplicadas y estados corruptos, que son riesgos típicos de un recurso compartido concurrente.

## Punto a revisar antes de entregar

En admisión general, las operaciones actuales cuentan `seat_ids.len()` pero no validan cada ID ni preservan el ID exacto seleccionado. Es suficiente para la simulación actual y no genera duplicados, pero un corrector que invoque `Booker` directamente podría exigir que `book_seats` devuelva los mismos IDs reservados.

La solución más robusta consiste en representar también las entradas generales con un estado atómico por ID, igual que las numeradas. Así se validan los IDs y se realiza la misma transición CAS `LIBRE → SELECCIONADA → VENDIDA`.

## Relación con la teoría

El fichero trata una condición de carrera sobre las plazas del recinto. La teoría distingue la seguridad (que nunca se venda dos veces un recurso) de la vivacidad (que la venta llegue a terminar). Aquí el CAS protege la seguridad sin serializar todo el recinto con un único mutex.

Referencias: [Tema 1: conceptos fundamentales](https://www.dlsi.ua.es/assignatures/pc/docs/teoria/s1/) y [Tema 4: mecanismos de sincronización](https://www.dlsi.ua.es/assignatures/pc/docs/teoria/s4/).
