# `cas.rs`: transición atómica de grupos de butacas

## Papel del fichero

`cas.rs` es un módulo auxiliar privado de `booking.rs`. No define una parte nueva de la API: concentra la operación delicada que cambia varias butacas numeradas de una vez.

Como `booking.rs` lo carga con:

```rust
#[path = "cas.rs"]
mod cas;
```

**debe incluirse en el directorio `p3` de entrega** junto con los ficheros principales.

## `cambiar_todas`

La función intenta transformar todas las posiciones indicadas desde un estado `desde` a un estado `hasta`.

Ejemplos de invocación:

```text
LIBRE         → SELECCIONADA  seleccionar
SELECCIONADA  → LIBRE         cancelar
SELECCIONADA  → VENDIDA       confirmar compra
```

Para cada posición se ejecuta `compare_exchange`:

- Si el valor observado es `desde`, se publica `hasta`.
- Si no lo es, otro hilo ya modificó esa plaza: la operación falla.

## Rollback seguro

Cuando falla una posición, las anteriores se revierten con otro CAS (`hasta → desde`), no con un `store` incondicional.

```text
Hilo A: cambia S1-10 de LIBRE a SELECCIONADA
Hilo B: observa SELECCIONADA y actúa sobre ese estado
Hilo A: falla al reservar S1-11
```

Un `store(LIBRE)` de A podría destruir el cambio legítimo de B. El CAS de rollback solo revierte si la plaza sigue exactamente en el estado que A había publicado.

## Garantía que ofrece

La función proporciona una semántica práctica de «todo o nada» para el lote:

- éxito: todas las butacas del lote cambian de estado;
- fallo: intenta devolver las que ya había cambiado a su estado anterior sin sobrescribir cambios ajenos.

`booking.rs` actualiza el contador de disponibles únicamente después de un éxito completo; así el contador no refleja una reserva parcial.

## Precondiciones

`booking.rs` debe entregar índices válidos, ordenados y sin duplicados. La conversión de ID a índice pertenece al motor de reservas, no a este módulo de bajo nivel.

## Relación con la teoría

CAS es una primitiva atómica de hardware. Aquí reemplaza una sección crítica global por exclusión mutua localizada: solo compiten los hilos que intentan cambiar la misma butaca.

Referencia: [Tema 1: exclusión mutua y condiciones de carrera](https://www.dlsi.ua.es/assignatures/pc/docs/teoria/s1/).
