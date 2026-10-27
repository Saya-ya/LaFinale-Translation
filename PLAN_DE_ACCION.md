# Plan de acción: parcheador de Script.pak y SolfegeLF.exe

**Estado:** repositorio Rust/web iniciado; validador de texto y generador
**diagnóstico** del EXE implementados. Ningún generador de PAK de producción
ni editor/instalador implementado.
**Objetivo:** convertir los JSON de traducción ya existentes en un `Script.pak`
con texto español y saltos correctos, producir un `SolfegeLF.exe` compatible
sin distribuir archivos originales del juego, y ofrecer una instalación
reversible. Los espacios deben ser **U+0020**, nunca U+3000.

## 1. Evidencia actual y problemas pendientes

- El motor borra espacios ASCII en un preprocesador; el EXE de prueba
  `SolfegeLF_SPACE_FIX.exe` evita ese borrado y se verificó en capturas.
- `SolfegeLF_LIMIT_TEST.exe` muestra hasta **52 bytes CP932 por renglón**
  sin sobrepasar el slot de 53 bytes incluido el NUL. Una captura confirmó
  50 caracteres ASCII visibles y que el sufijo después del byte 52 se corta
  si no se introduce un salto. **No mide el ancho real del recuadro**.
- El motor reconoce los marcadores literales `\n` (otro renglón) y `\p`
  (cambio de página). La captura del prólogo confirmó `\n`; **aún no se ha
  confirmado `\p`, el paso de cinco renglones ni la partida completa**.
- El prototipo `TranslationLaFinale/pruebas_espacios_ascii/build_auto_wrap_pak.py`
  procesa los JSON, deja intacto el menú y produce un PAK separado con 3.577
  cadenas divididas en tramos de hasta **48 bytes CP932**. Verificó por
  reextracción las 118 entradas y las 9.673 cadenas traducidas, **pero no
  se ha probado visualmente ese PAK completo**. No convertirlo directamente
  en un parche oficial.
- El menú (`scr00000.kgo`) usa otra ruta de presentación y requiere un
  análisis/prueba específicos; tampoco se ha demostrado que las tres rutas
  de texto de escena compartan una única anchura.

## 2. Arquitectura acordada: web local en Rust

**Aplicación web Rust**, siguiendo el patrón `core`/`web` de StrawTraduccion:
`lafinale-core` concentra reglas CP932 y el port del parche PE de prueba;
SF03, HyPack y layout siguen pendientes. `lafinale-web` expone una interfaz local Axum en
`127.0.0.1`, nunca abierta por defecto a otras máquinas. El formulario
actual previsualiza capacidad y permite crear un EXE diagnóstico en `output/`,
pero no persiste traducciones ni parchea la instalación.

Los prototipos Python anteriores son referencia para portar el algoritmo
correcto, **no** una dependencia de la aplicación final. Cada módulo Rust
debe contrastarse con pruebas independientes y round-trips de datos aportados
localmente. Añadir después SQLite/FTS5 y un editor web; no afirmar que ya
existen. Una CLI Rust auxiliar puede reutilizar el núcleo, pero el flujo de
traducción previsto es la web, no una GUI nativa `egui`.

Estructura de partida y futuras áreas:

```text
LaFinalePatch/
  Cargo.toml                   # workspace Rust
  crates/lafinale-core/        # validador CP932 + EXE diagnóstico; después HyPack/SF03
  crates/lafinale-web/         # Axum + interfaz web local
  tests/                       # futuras pruebas de integración sin binarios públicos
  docs/                        # futuras notas de formatos y pruebas Windows
  translations/                # solo JSON autorizados para distribuir, más adelante
```

## 3. Fases y condiciones para avanzar

### Fase A — cerrar el comportamiento del juego (bloqueante)

1. Probar en Windows el PAK `traduccion_saltos_auto/Script.pak` con
   `SolfegeLF_LIMIT_TEST.exe`: principio, línea 52+, páginas sucesivas,
   avance normal/rápido, auto/skip, historial, guardar/cargar y mezcla
   japonés+ASCII. Confirmar que ninguna línea posterior desaparece.
2. Preparar una **prueba pequeña y aislada de `\p`** cuando un mensaje o
   página ocupe cinco filas. Verificar que el sufijo aparece después del
   cambio de página y no borra texto antes de poder leerlo. Si el motor no
   pagina solo, no publicar un PAK que inserte únicamente `\n`.
3. Trazar y probar el menú por separado, sin aplicar automáticamente el
   algoritmo de diálogo a `scr00000.kgo`.
4. Definir el ancho utilizable: el prototipo actual usa 48 **bytes** como
   margen fijo, no medidas tipográficas. Evaluar si basta ese límite para
   lectura correcta o si hace falta medir con la fuente/HDC reales y
   conservar la velocidad/revelado. No ampliar slots ni constantes del EXE
   sin revisar todos los lectores y límites relacionados.

**Salida de fase:** capturas y casos de regresión reproducibles de renglón,
página, menú, japonés y textos mixtos. Si falla, corregir el prototipo,
no seguir a empaquetado final.

### Fase B — generador reproducible de PAK

1. Entrada explícita: `Script.pak` **original verificado** y carpeta de JSON.
   Seleccionar los 117 scripts por nombre e índice, preservar bytecode,
   índices y 118 entradas originales. Los campos `spanish` vacíos mantienen
   texto japonés; no aplicar `.strip()` que borre espacios intencionales.
2. Diseñar `layout` puro: contar **bytes codificados CP932**, no caracteres
   Unicode; saltar preferentemente en espacios U+0020 antes del límite;
   no partir pares DBCS, ni secuencias de control, ni palabras sin comprobar
   cómo deben romperse. Generar comandos `\n`/`\p` únicamente tras superar
   fase A. Conservar el resto y el orden de cada frase; menú con política
   separada. El JSON fuente permanece intacto.
3. Validar caracteres no representables en CP932 (y la política actual de
   normalización de acentos) mediante informe explícito; no reemplazar con
   `?` silenciosamente. Rechazar U+3000 en campos traducidos.
4. Reempaquetar en el orden original, verificar CRC/TOC, volver a extraer
   **todas** las entradas y comparar las traducidas con el resultado
   esperado. Pruebas de límites 47/48/52 bytes, CP932 mixto, marcador al
   final, nombre largo y cadenas sin espacios. Informe de filas/páginas y
   posibles líneas pendientes de revisión humana.

**Salida de fase:** `Script.pak` de **salida en otra carpeta**, hash/manifest
y reporte de conservación de cadenas; ningún cambio en el juego instalado.

### Fase C — parcheador de EXE, estricto por versión

1. Entrada: EXE NoDVD compatible aportado por el usuario. Verificar hash,
   PE32, ImageBase, secciones y firmas de los bytes antes de escribir.
2. Integrar los parches **SBCS/DBCS**, conservación de U+0020 y límite
   con guardas de 53 bytes. No copiar offsets a otras versiones del juego.
   Revisar si hay que corregir revelado/clip según fase A; el EXE
   `LIMIT_TEST` es solo una base diagnóstica, no un binario final.
3. Salida: EXE nuevo con checksum PE, manifiesto de offsets/hashes y
   verificación de que únicamente cambiaron los rangos autorizados.

**Salida de fase:** EXE reproducible desde el original verificado y
compatible con el PAK construido en fase B; regresión en Windows.

### Fase D — instalación reversible y publicación

1. Modo por defecto `--output <carpeta>`: construir fuera de la instalación.
   Modo instalar **solo con confirmación**: comprobar EXE y PAK actuales,
   guardar respaldo con nombres claros y no sobrescribir respaldo previo.
   Escribir primero a temporales, verificar y sustituir al final; ofrecer
   restauración y explicar fallos sin dejar un par EXE/PAK incompatible.
2. Automatizar tests unitarios y end-to-end de extracción, inyección,
   hashes, espacio U+0020, wrapping y conservación de todos los índices.
   Mantener tests de humo manuales en Windows (juego con locale/fuentes
   japonesas) y una matriz de versiones soportadas.
3. Publicar **herramienta y, si corresponde, JSON con permiso/licencia**,
   nunca el EXE/PAK original ni archivos del juego incorporados al repo.
   Antes de distribuir traducciones o releases, revisar derechos sobre
   traducciones y dependencias y escoger una licencia explícita para el
   código. Documentar instrucciones de respaldo y reversión.

## 4. Interfaz web prevista

Flujo previsto de la web local: escoger directorio del juego y
JSON → comprobar versiones/hashes → previsualizar cantidad de scripts,
cadenas transformadas y advertencias → elegir destino → crear ambos archivos
en staging → validar → **pedir confirmación** antes de instalar → mostrar
copias de seguridad y opción de restaurar. No ofrecer un botón «parchear»
que oculte los errores de texto, la compatibilidad del EXE o los pasos de
respaldo.

**Próxima decisión:** revisar las capturas del PAK con saltos automáticos y
probar `\p`. Solo entonces fijar el contrato de `layout` y elegir cómo
empaquetar/visualizar el instalador. El repositorio puede desarrollarse antes
de esas pruebas, pero no etiquetar el parche como utilizable hasta pasarlas.
