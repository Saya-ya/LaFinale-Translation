# LaFinale Translation

Aplicación web **local** en Rust para preparar la traducción de *Solfège ~La finale~* (PC): revisar textos, controlar su capacidad real en CP932 y, más adelante, construir `Script.pak` y parchear `SolfegeLF.exe` **a partir de los archivos que aporte cada usuario**.

Inspirada en la separación `core`/`web` de [StrawTraduccion](https://github.com/Saya-ya/StrawTraduccion), pero con formatos y límites propios de este juego. Proyecto fan, gratuito y no oficial.

> **Estado: prototipo inicial, NO es un parche instalable.** La web solo valida y previsualiza texto y prepara sus carpetas locales. No extrae, importa, guarda traducciones, genera PAK ni modifica ejecutables todavía. Los experimentos anteriores se mantienen fuera de este repositorio.

## Por qué hay límites específicos

El motor usa archivos **HyPack v3** (`Script.pak`) y scripts **SF03** (`.kgo`) cuyos índices de texto deben mantenerse. Las cadenas se codifican en **CP932**. El EXE original elimina espacios ASCII y el compositor del diálogo usa slots de **53 bytes**, incluido el terminador NUL: **52 bytes útiles**, no 52 caracteres Unicode.

Las pruebas con Ghidra y capturas demostraron que el juego reconoce el marcador literal `\n` para pasar a otra línea y que un EXE experimental puede mostrar más de 26 caracteres ASCII sin sobrepasar 52 bytes. **Todavía no está validado** el cambio de página `\p`, el caso de cinco filas llenas, la presentación del menú ni una partida completa. La web muestra de momento un umbral conservador de **48 bytes CP932 por tramo**; es una comprobación binaria, **no una medida de píxeles ni una garantía visual**. Se usan espacios normales U+0020, nunca espacios de ancho completo U+3000.

## Ejecutar el prototipo

Requiere Rust estable y Cargo. Desde la raíz de este repositorio:

```bash
cargo run -p lafinale-web
```

Abre <http://127.0.0.1:8080>. El servidor escucha únicamente en `127.0.0.1`. Puedes pegar una frase en el formulario y ver cuántos bytes CP932 ocuparía, cuántos quedarían en un tramo conservador y si excede la capacidad física del slot. También está disponible `GET /health`. No se sube texto a servicios externos ni se guardan las frases ingresadas.

### Primera ejecución y API

Inicia el servidor **desde la raíz del repositorio**. Antes de escuchar peticiones, crea solo las carpetas ausentes:

```text
originales/          # aquí pondrá el usuario Script.pak y SolfegeLF.exe
work/                # datos de trabajo futuros; aún no se escriben
work/translations/   # JSON locales futuros; aún no se importan
output/              # PAK y EXE construidos futuros; aún no se generan
```

Las tres áreas de datos están ignoradas por Git. Si una de estas rutas ya es un archivo o un enlace en lugar de una carpeta, la aplicación **se detiene con error** en vez de reemplazarla. Los originales existentes nunca se sobrescriben. Si vas a lanzar el binario desde otra carpeta, define `LAFINALE_HOME` apuntando a la raíz del proyecto.

| Ruta | Función actual |
| --- | --- |
| `GET /health` | Respuesta simple `ok` (compatibilidad con el prototipo). |
| `GET /api/health` | Salud en JSON. |
| `GET /api/status` | Estado de las cuatro carpetas y presencia de `originales/Script.pak` y `originales/SolfegeLF.exe`. |

Ejemplo de respuesta de `/api/status` en una instalación nueva:

```json
{
  "directories": [
    {"path":"originales","exists":true},
    {"path":"work","exists":true},
    {"path":"work/translations","exists":true},
    {"path":"output","exists":true}
  ],
  "inputs": {"script_pak_present":false,"exe_present":false,"validated":false}
}
```

`present` solo comprueba si existe un archivo regular. **No implica que sea una versión compatible**: la lectura de cabeceras y hashes del EXE/PAK se implementará más adelante. Los endpoints son de consulta: no aceptan rutas del cliente ni instalan parches.

Para desarrollo:

```bash
cargo fmt --all --check
cargo test --workspace
cargo check --workspace
```

## Diseño previsto

```text
crates/
  lafinale-core/     # validación CP932 y, después, SF03/HyPack/EXE
  lafinale-web/      # Axum; interfaz local para importar, editar y construir
PLAN_DE_ACCION.md    # fases, límites y pruebas pendientes
```

1. Validar en Windows saltos de renglón/página, menú, historial y mezcla japonés/ASCII.
2. Importar `Script.pak` original y JSON; conservar índices y bytecode; editor web con estados de capacidad **en bytes**, revisión de saltos y traducción sin alterar los originales.
3. Reempaquetar y verificar por reextracción las 118 entradas; generar `Script.pak` nuevo fuera de la instalación.
4. Parchear **solo un EXE de versión verificada** para SBCS/DBCS, espacios ASCII y límites de memoria; exigir firma/hash y emitir archivo nuevo con reporte.
5. Añadir instalación opcional con respaldo/confirmación y restauración. Sin resultados silenciosamente truncados.

Detalles y criterios de aceptación: [PLAN_DE_ACCION.md](PLAN_DE_ACCION.md).

## Aviso legal y archivos del juego

Cada usuario debe aportar sus propios archivos originales. Este repositorio **no incluye** EXE, PAK, scripts extraídos, imágenes, voces ni versiones parcheadas del juego. No publiques una instalación, binarios derivados ni datos de prueba obtenidos del juego. Los derechos de *Solfège* y sus recursos pertenecen a sus titulares. Los JSON de traducción solo se incorporarán cuando se revisen su procedencia y condiciones de distribución; por ahora tampoco están aquí.

Proyecto: [Saya-ya/LaFinale-Translation](https://github.com/Saya-ya/LaFinale-Translation) · Referencia de interfaz: [StrawTraduccion](https://github.com/Saya-ya/StrawTraduccion)
