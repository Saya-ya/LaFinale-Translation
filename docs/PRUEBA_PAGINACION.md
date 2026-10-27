# Prueba aislada: `\p` en el comienzo del prólogo

**No es el parche final.** Prueba exclusivamente cómo responde el juego al
marcador de cambio de página. El builder ordinario sigue rechazando los
marcadores manuales y diálogos que necesitan cinco filas.

1. Requiere `output/Script.pak` diagnóstico, SHA-256
   `8234861e352d578d7ec1f3e1e1c277faf5cff20e66ef15960b01a7b989c89453`.
   No modifica `originales/`, `work/translations/` ni ese PAK de salida.
2. Con la API local en marcha, ejecutar **una sola vez**:

   ```bash
   curl -X POST http://127.0.0.1:8080/api/pak/page-probe
   ```

   La respuesta contiene la ruta y SHA-256 del PAK nuevo en
   `output/prueba_paginacion/Script.pak`. Si ya existe, la API responde 409;
   no lo sobrescribe.
3. En la primera frase del prólogo (`scr00001.kgo`, índice 0), el PAK habitual
   contiene exactamente:

   ```text
   [Ejecutivo]  "... Respecto al proximo\ncontrato..."
   ```

   El PAK experimental sustituye **solo ese** `\n` literal por `\p` literal:

   ```text
   [Ejecutivo]  "... Respecto al proximo\pcontrato..."
   ```

   La longitud de cada tramo queda por debajo de 48 bytes CP932. El PAK
   completo conserva orden y datos de las 118 entradas; la API vuelve a
   calcular el CRC global. No se cambia `SolfegeLF_DIAGNOSTIC.exe`.

## Prueba en Windows

- Haz una copia de seguridad **fuera del directorio del juego** de tu
  `Script.pak` y `SolfegeLF.exe` instalados. No reemplaces ni pierdas esas
  copias. Usa el EXE **diagnóstico** compatible con espacios ASCII y 52 bytes
  CP932, no el EXE original sin parche.
- Copia **manual y temporalmente**
  `output/prueba_paginacion/Script.pak` como `Script.pak` en la instalación.
  La API no instala ningún archivo del juego.
- Comienza una partida nueva desde el prólogo. Comprueba si la primera frase
  muestra «Respecto al proximo» y espera tu avance; al continuar, debe
  aparecer «contrato...» en **una página nueva**, sin perder la continuación.
  Revisa también la segunda y tercera intervenciones, historial, avance normal
  y automático. Una captura de ambas páginas permitirá confirmar o refutar el
  comportamiento; **aún no se ha probado visualmente**.
- Al terminar, restaura el PAK y EXE instalados desde tus respaldos. El PAK
  de prueba y los JSON permanecen ignorados por Git.

Si el motor no pagina, muestra `p` literalmente, omite «contrato...» o corta
el diálogo, no activar cambios de página automáticos ni distribuir este PAK.
