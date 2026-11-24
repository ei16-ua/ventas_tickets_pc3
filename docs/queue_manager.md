# `queue_manager.rs`: coordinación de compradores

## Papel del fichero

Este módulo implementa el trait `QueueManager`: crea el motor de reservas, lanza sesiones de compra concurrentes y reúne sus tickets.

`SequentialQueueManager` es un adaptador que delega en `ConcurrentQueueManager`. Su presencia permite que el `main.rs` original, que crea un gestor con ese nombre, ejecute el algoritmo concurrente sin modificar un fichero protegido.

## Ejecución de `ConcurrentQueueManager::run`

1. Lee la capacidad total. Si es cero, termina inmediatamente.
2. Calcula un número de workers limitado por `MAX_THREADS`.
3. Crea workers dentro de `thread::scope`.
4. Cada worker clona `ConcurrentBooker`; el clon comparte el mismo `Arc<ConcurrentVenue>`.
5. Cada worker llama repetidamente a `simulate_booking` mientras queden entradas sin vender.
6. Los tickets logrados localmente se devuelven al hilo padre con `join` y se agregan a `sold_tickets`.

```text
QueueManager
  └─ worker 0 ─┐
  └─ worker 1 ─┼─> Arc<ConcurrentVenue> ─> estados atómicos
  └─ worker N ─┘
```

## Por qué usa `thread::scope`

Los workers pueden tomar prestados datos del método `run` sin requerir que todo sea `'static`. Al terminar el bloque de `scope`, todos se han unido. Por ello `run` no devuelve tickets antes de que finalicen los compradores.

## Finalización y control de contención

`remaining_seats()` usa el total de ventas consolidadas. Cuando alcanza cero, los workers abandonan el bucle.

En la fase final, `active_workers` limita progresivamente los hilos que siguen intentando comprar. De esta forma se evita que cientos de workers compitan por muy pocas entradas, pero se mantienen como mínimo `MIN_TAIL_WORKERS` para no convertir la cola final en una ejecución secuencial.

No hay espera activa intensa: los workers retirados temporalmente duermen 200 microsegundos. Esto reduce consumo inútil de CPU cuando la probabilidad de conseguir una plaza ya es baja.

## Propiedades buscadas

- **Seguridad:** el gestor solo concatena tickets que el `Booker` ya confirmó.
- **Vivacidad:** los hilos se unen y no retienen cerrojos; las reservas canceladas vuelven a estar disponibles.
- **Rendimiento:** cada worker acumula sus propios tickets y se evita un `Mutex<Vec<String>>` compartido en cada venta.

## Cómo comprobarlo

`test_full_profile` verifica que el número de tickets coincide con la capacidad, no hay duplicados y el recinto final no conserva plazas disponibles. Conviene ejecutar varias veces:

```bash
cargo test --release --bin tickets
```

La concurrencia es no determinista: que una sola ejecución pase no demuestra todos los entrelazados posibles.

## Relación con la teoría

La cola de compradores ilustra el orden parcial de la concurrencia: los resultados individuales llegan en un orden no predecible, pero el resultado global debe conservar las invariantes. La teoría remarca que ausencia de *deadlock*, *livelock* e inanición son propiedades de vivacidad que deben diseñarse, no asumirse.

Referencia: [Tema 1: seguridad y vivacidad](https://www.dlsi.ua.es/assignatures/pc/docs/teoria/s1/).
