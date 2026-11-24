# `venue.rs`: modelo del recinto y estado compartido

## Modelo secuencial

Las estructuras `Venue`, `Sector` y `Seat` representan el modelo de dominio original:

- `Venue` agrupa sectores y calcula capacidad, plazas disponibles y vendidas.
- `Sector` contiene sus IDs de asiento, su código y si las localidades son numeradas.
- `SeatState` expresa `Available`, `Selected` o `Booked`.

Este modelo es cómodo para mostrar el resultado final y para la implementación secuencial, pero no debe ser mutado directamente por varios threads.

## Modelo concurrente

`ConcurrentVenue::from_venue` crea una copia diseñada para compartirse mediante `Arc`:

| Elemento | Sectores numerados | Admisión general |
| --- | --- | --- |
| Estado de cada plaza | `Vec<AtomicU8>` | Contadores atómicos |
| Disponibilidad | `ContadorIsize` | `Contador` |
| Venta definitiva | estado `VENDIDA` | contador `booked` |
| IDs | vector de IDs | vector de IDs y cursor de emisión |

Los tres valores atómicos de una butaca numerada son:

```text
LIBRE (0) → SELECCIONADA (1) → VENDIDA (2)
```

## Contadores y *false sharing*

`Contador` y `ContadorIsize` están alineados a 64 bytes. La intención es separar contadores muy modificados para que varios núcleos no invaliden constantemente la misma línea de caché.

`ContadorIsize` se emplea para plazas numeradas porque, durante un rollback CAS concurrente, puede existir una observación transitoria. Su método `get` no expone valores negativos como capacidad disponible.

## Conversión inversa: `snapshot`

`ConcurrentVenue::snapshot` reconstruye un `Venue` convencional a partir de los estados atómicos. Se usa en `get_venue` una vez que el gestor ha terminado, para que las pruebas puedan verificar:

- capacidad vendida completa;
- cero plazas disponibles;
- estado final por sector.

## Invariante principal

Para cada sector se debe preservar:

```text
libres + seleccionadas + vendidas = capacidad
```

Además, `total_booked` debe coincidir con la suma de ventas definitivas del recinto. Estas igualdades son la base de la comprobación de seguridad del algoritmo.

## Relación con la teoría

El recinto es memoria compartida. La teoría advierte que una operación aparentemente simple de lectura-modificación-escritura no es atómica: por eso el estado se modela con atómicos y transiciones controladas, en lugar de con campos mutables ordinarios.

Referencias: [Tema 1: exclusión mutua y condiciones de carrera](https://www.dlsi.ua.es/assignatures/pc/docs/teoria/s1/) y [Tema 4: sincronización](https://www.dlsi.ua.es/assignatures/pc/docs/teoria/s4/).
