# `algorithm_name.txt`: identificación de la solución

## Qué debe contener

El fichero entregable `algorithm_name.txt` debe tener **una única línea** que identifique el algoritmo. No debe contener Markdown, explicación adicional ni varias líneas.

Una propuesta coherente con la implementación actual es:

```text
Lock-Free Per-Seat CAS with Scoped Worker Pool
```

## Qué describe ese nombre

- **Lock-Free Per-Seat CAS:** las plazas numeradas se gestionan individualmente con `compare_exchange`; no existe un mutex global del recinto.
- **Scoped Worker Pool:** `ConcurrentQueueManager` lanza workers en `thread::scope`, comparte el motor mediante `Arc` y reúne resultados al final.

El nombre debe corresponder con el código que se entregue. Si se reemplaza el diseño por otro —por ejemplo, mutex por sector— habría que actualizar esta línea para no describir un algoritmo distinto.

## Paquete final

El directorio `p3` debe contener los ficheros fuente requeridos y los auxiliares que estos importan:

```text
p3/
├── booking.rs
├── queue_manager.rs
├── venue.rs
├── cas.rs
└── algorithm_name.txt
```

Los documentos de `docs/` son material de estudio y no deben copiarse al archivo `p3.tgz`, salvo que el profesor solicite documentación adicional.

## Relación con la teoría

El nombre resume las dos dimensiones de la práctica: seguridad mediante transiciones atómicas por recurso y vivacidad mediante un conjunto de workers que termina al agotarse el aforo.

Referencia: [Tema 1: propiedades de seguridad y vivacidad](https://www.dlsi.ua.es/assignatures/pc/docs/teoria/s1/).
