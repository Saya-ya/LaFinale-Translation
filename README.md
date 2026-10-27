# LaFinale Translation

Aplicación web **local** en Rust para preparar la traducción de *Solfège ~La finale~* (PC): revisar textos, controlar su capacidad real en CP932 y, más adelante, construir `Script.pak` y parchear `SolfegeLF.exe` **a partir de los archivos que aporte cada usuario**.

Inspirada en la separación `core`/`web` de [StrawTraduccion](https://github.com/Saya-ya/StrawTraduccion), pero con formatos y límites propios de este juego. Proyecto fan, gratuito y no oficial.

> **Estado: prototipo diagnóstico, NO es un parche instalable.** La web muestra el estado local, importa a JSON los diálogos del PAK original probado y genera **PAK y EXE de prueba independientes**. No instala archivos en el juego. Los experimentos anteriores se mantienen fuera de este repositorio.

El núcleo Rust lee HyPack v3/Cocotte, valida CRC y descomprime las 118 entradas del PAK original probado. Comprueba los 117 scripts SF03 y genera JSON por índice sin modificar el original.

El núcleo puede reconstruir un PAK **diagnóstico** desde esos JSON: solo usa traducciones no vacías, conserva bytecode e índices, normaliza caracteres españoles incompatibles con CP932 de manera explícita y limita los renglones de diálogo a 48 bytes. Verifica las 118 entradas por reextracción. Si una traducción requiere cinco filas o contiene un control manual `\p`/`\n`, **rechaza toda la construcción** hasta verificar paginación en el juego.

## Por qué hay límites específicos

El motor usa archivos **HyPack v3** (`Script.pak`) y scripts **SF03** (`.kgo`) cuyos índices de texto deben mantenerse. Las cadenas se codifican en **CP932**. El EXE original elimina espacios ASCII y el compositor del diálogo usa slots de **53 bytes**, incluido el terminador NUL: **52 bytes útiles**, no 52 caracteres Unicode.

Las pruebas con Ghidra y capturas demostraron que el juego reconoce el marcador literal `\n` para pasar a otra línea y que un EXE experimental puede mostrar más de 26 caracteres ASCII sin sobrepasar 52 bytes. **Todavía no está validado** el cambio de página `\p`, el caso de cinco filas llenas, la presentación del menú ni una partida completa. El validador Rust conserva un umbral experimental de **48 bytes CP932 por tramo**; aún no hay editor web de textos ni medida de píxeles o garantía visual. Se usan espacios normales U+0020, nunca espacios de ancho completo U+3000.

## Ejecutar el prototipo

Requiere Rust estable y Cargo. Desde la raíz de este repositorio:

```bash
cargo run -p lafinale-web
```

Abre <http://127.0.0.1:8080>. El servidor escucha únicamente en `127.0.0.1`. La barra superior permite navegar entre **Inicio** (archivos, salidas, JSON presentes e idiomas japonés → español), **Importar** (extrae 117 JSON originales) y **Parchear** (genera PAK y EXE diagnósticos por separado). La interfaz no depende de fuentes, scripts ni servicios externos. No se sube texto a servicios externos.

### Primera ejecución y API

Inicia el servidor **desde la raíz del repositorio**. Antes de escuchar peticiones, crea solo las carpetas ausentes:

```text
originales/          # aquí pondrá el usuario Script.pak y SolfegeLF.exe
work/                # datos de trabajo futuros; aún no se escriben
work/translations/   # JSON extraídos localmente; no se suben a Git
output/              # PAK y EXE diagnósticos, separados de los originales
```

Las tres áreas de datos están ignoradas por Git. Si una de estas rutas ya es un archivo o un enlace en lugar de una carpeta, la aplicación **se detiene con error** en vez de reemplazarla. Los originales existentes nunca se sobrescriben. Si vas a lanzar el binario desde otra carpeta, define `LAFINALE_HOME` apuntando a la raíz del proyecto.

| Ruta | Función actual |
| --- | --- |
| `GET /health` | Respuesta simple `ok` (compatibilidad con el prototipo). |
| `GET /api/health` | Salud en JSON. |
| `GET /api/status` | Estado de carpetas y entradas, recuento de JSON, campos `spanish` no vacíos y presencia de las salidas. |
| `POST /api/exe/build` | Aplica **solo** al EXE NoDVD de SHA-256 `a7e4999ff54e56499130a45427ab0273bc6b9d475e03c93f493cb6b592a13a79`; crea `output/SolfegeLF_DIAGNOSTIC.exe` sin sobrescribir. |
| `POST /api/import` | Solo PAK original con SHA-256 `b5666dc0155b7534b9455100b39b6e8c40e28051f47aa208517a2b645e1bb40e`; extrae los 117 JSON a `work/translations/` **vacía**, sin sobrescribir. |
| `POST /api/pak/build` | Requiere el mismo PAK original y los 117 JSON (al menos una traducción). Crea `output/Script.pak` **solo si no existe**; devuelve hash, número de frases y normalizaciones. |
| `POST /api/pak/page-probe` | **Experimento aislado:** exige el `output/Script.pak` diagnóstico de hash conocido y crea `output/prueba_paginacion/Script.pak` con un único `\n` sustituido por `\p` en `scr00001.kgo`, cadena 0. Nunca sobrescribe. |

Ejemplo de respuesta de `/api/status` en una instalación nueva:

```json
{
  "directories": [
    {"path":"originales","exists":true},
    {"path":"work","exists":true},
    {"path":"work/translations","exists":true},
    {"path":"output","exists":true}
  ],
  "inputs": {"script_pak_present":false,"exe_present":false,"validated":false},
  "workspace": {"translation_json_count":0,"filled_spanish_count":0,"diagnostic_exe_present":false,"output_pak_present":false}
}
```

`present` solo comprueba si existe un archivo regular. El recuento de `filled_spanish_count` examina los campos `spanish` no vacíos de JSON legibles con índices consecutivos; devuelve `null` si hay JSON ilegibles, mal formados o una carpeta inaccesible. **No confirma que el texto sea español válido ni compatible con CP932**: `/api/status` no compara los JSON contra el PAK ni verifica hashes. Los endpoints de estado son de consulta: no aceptan rutas del cliente ni instalan parches.

La importación verifica el hash del PAK, CRC global y por entrada, descomprime Cocotte y comprueba los 117 scripts SF03 antes de escribir. Extrae `japanese`, `index`, `rel_offset`, `raw_len` y un campo `spanish` vacío; **no detecta ni añade traducciones**. Rechaza entradas de otra versión y no pisa JSON previos. Si falla una escritura, retira solo los archivos creados por esa petición. `POST /api/import` no acepta rutas del cliente.

Al rellenar campos `spanish` en tus JSON locales, `POST /api/pak/build` genera el PAK por separado. Copia intactas las entradas sin traducción y rechaza cualquier cambio al índice, texto japonés o identificador original. Divide diálogos a **48 bytes CP932 por renglón** con marcadores literales `\n`; conserva espacios ASCII salvo el espacio que sustituye cada salto. No aplica este formato al menú (`scr00000.kgo`), que usa otra interfaz. Normaliza explícitamente vocales acentuadas y macrones (`á→a`, `ō→o`, `ū→u`…), `ñ→n`, `«»→"`, y retira `¿¡`; el recuento de caracteres afectados se muestra en la respuesta. **No reemplaza caracteres desconocidos por `?` en silencio**. Por estas correcciones la salida puede tener un hash distinto del PAK de prueba Python anterior. Ninguna validación binaria reemplaza la prueba visual de menú, páginas, historial y partida completa.

El endpoint de construcción es la excepción: verifica el **hash completo del EXE NoDVD**, estructura PE32 y firmas de bytes, aplica SBCS/DBCS, conserva los espacios ASCII y amplía el límite **diagnóstico** de 26 a 52 bytes CP932 por slot. Comprueba los hashes de cada fase contra los prototipos. Rechaza una versión distinta con HTTP 422, una entrada ausente con 404 y una salida existente con 409. No acepta rutas proporcionadas por el cliente. Ejemplo, después de depositar el EXE propio:

```bash
curl -X POST http://127.0.0.1:8080/api/exe/build
```

**Advertencia:** el motor seguirá truncando líneas superiores a 52 bytes; el parche no mide píxeles, no pagina automáticamente y no soluciona la UI del menú. El español se agrega mediante el futuro `Script.pak` traducido, **no incrustando frases en el EXE**. La salida no sustituye `SolfegeLF.exe` de la instalación ni debe presentarse como parche final. La API es local: la operación de escritura exige `Host: 127.0.0.1:8080` y rechaza solicitudes web de otro origen.

Para comprobar manualmente el cambio de página `\p` sin modificar JSON ni la salida normal, consulta [docs/PRUEBA_PAGINACION.md](docs/PRUEBA_PAGINACION.md). Esta prueba **no habilita** el paginado automático en el constructor ordinario.

Para desarrollo:

```bash
cargo fmt --all --check
cargo test --workspace
cargo check --workspace
```

## Diseño previsto

```text
crates/
  lafinale-core/     # validación CP932, lectura SF03/HyPack y EXE diagnóstico
  lafinale-web/      # Axum; tres vistas locales, API de estado y EXE diagnóstico
PLAN_DE_ACCION.md    # fases, límites y pruebas pendientes
```

1. Validar en Windows saltos de renglón/página, menú, historial y mezcla japonés/ASCII.
2. Importar `Script.pak` original y JSON; conservar índices y bytecode; editor web con estados de capacidad **en bytes**, revisión de saltos y traducción sin alterar los originales.
3. Reempaquetar y verificar por reextracción las 118 entradas; generar `Script.pak` nuevo fuera de la instalación.
4. Evolucionar el parche EXE diagnóstico actual, limitado a una versión verificada (SBCS/DBCS, espacios ASCII y 52 bytes), hacia una solución comprobada visualmente antes de integrarlo como parche final.
5. Añadir instalación opcional con respaldo/confirmación y restauración. Sin resultados silenciosamente truncados.

Detalles y criterios de aceptación: [PLAN_DE_ACCION.md](PLAN_DE_ACCION.md).

## Aviso legal y archivos del juego

Cada usuario debe aportar sus propios archivos originales. Este repositorio **no incluye** EXE, PAK, scripts extraídos, imágenes, voces ni versiones parcheadas del juego. No publiques una instalación, binarios derivados ni datos de prueba obtenidos del juego. Los derechos de *Solfège* y sus recursos pertenecen a sus titulares. Los JSON de traducción solo se incorporarán cuando se revisen su procedencia y condiciones de distribución; por ahora tampoco están aquí.

Proyecto: [Saya-ya/LaFinale-Translation](https://github.com/Saya-ya/LaFinale-Translation) · Referencia de interfaz: [StrawTraduccion](https://github.com/Saya-ya/StrawTraduccion)
