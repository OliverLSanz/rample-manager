# Ejemplos de configuración de una SD de Rample Turbo

Objetivo: confirmar cómo tu firmware codifica el modo de capas y la capa manual
seleccionada, para leer y actualizar esos ajustes sin alterar el resto del kit.
La app 1.1 guarda estos ajustes en el Mac; todavía no modifica los `.rpl` nativos.

## Preparación

1. Con el Rample **apagado**, extrae la SD y haz una copia completa en el Mac.
   Conserva ese respaldo original sin modificar.
2. Vuelve a insertar la SD con el módulo apagado y enciéndelo.
3. Anota **Rample Turbo**, la versión exacta que aparece en **Settings → INFO**
   y el nombre del kit que usarás, por ejemplo `A0`.
4. Escoge un kit con al menos tres WAV/capas en la voz 1. Conserva un listado de
   los nombres de todos sus WAV; no hace falta enviar el audio.
5. No cambies de kit durante las pruebas ni muevas los potenciómetros. Así cada
   comparación tendrá una única modificación intencionada.

## Cómo cambiar el modo y la capa

Para una voz:

1. Pulsa y suelta **ASSIGN**.
2. Pulsa su botón, por ejemplo **1**, para entrar en capas. En **MANUAL**, gira
   el encoder para escoger la capa.
3. Pulsa de nuevo el botón de la voz para acceder al modo de capas. Elige
   **MANUAL**, **RANDOM**, **CYCLIC**, **REVERSE CYCLIC** o **VELOCITY** con el encoder.
4. Vuelve al menú de ajustes y ejecuta **STORE** para guardar el kit.

El modo de capas por voz se guarda con **STORE**. El ajuste global
**Settings → LAYER** se guarda con **SAVE SETTINGS**. Son ajustes distintos.

## Seis capturas del kit

Crea en el Mac una carpeta, por ejemplo `Rample SD captures`, y dentro una
carpeta por captura. Después de cada guardado, **apaga el módulo**, extrae la SD,
conéctala al Mac y copia la carpeta completa `_save` dentro de la captura.
Expulsa la SD del Mac y vuelve a insertarla con el Rample apagado antes de seguir.

| Carpeta de captura | Acción en el Rample antes de copiar |
| --- | --- |
| `01-manual` | Todas las voces en MANUAL y su primera capa seleccionada. STORE y SAVE SETTINGS. |
| `02-random` | Cambia solo la voz 1 a RANDOM. STORE. |
| `03-cyclic` | Cambia solo la voz 1 a CYCLIC. STORE. |
| `04-reverse` | Cambia solo la voz 1 a REVERSE CYCLIC. STORE. |
| `05-velocity` | Cambia solo la voz 1 a VELOCITY. STORE. |
| `06-manual-layer2` | Devuelve la voz 1 a MANUAL, selecciona su segunda capa y ejecuta STORE. |

No sobrescribas las capturas anteriores. Deben quedar así:

```text
Rample SD captures/
├── notes.txt
├── 01-manual/
│   └── _save/
│       ├── A0.rpl          # el nombre real será el de tu kit
│       ├── settings.rpl
│       └── ...             # conserva todos los archivos, sin editar
├── 02-random/
│   └── _save/...
├── 03-cyclic/
│   └── _save/...
├── 04-reverse/
│   └── _save/...
├── 05-velocity/
│   └── _save/...
└── 06-manual-layer2/
    └── _save/...
```

En `notes.txt`, escribe el modelo, firmware, kit y nombres de sus WAV. Si la SD
no tiene `_save` o algún nombre difiere del ejemplo, copia los archivos reales
sin renombrarlos. También sirve copiar toda la SD en cada ronda si te resulta
más sencillo; basta con indicar dónde están las copias.

## Dos capturas adicionales del ajuste global

Estas completan la comprobación del valor por defecto que usa el módulo para
kits que nunca se hayan guardado con STORE:

1. Sin cambiar el kit ni sus voces, elige **Settings → LAYER → MANUAL**, ejecuta
   **SAVE SETTINGS** y copia `_save` a `07-global-manual`.
2. Cambia solo ese ajuste global a **RANDOM**, ejecuta **SAVE SETTINGS** y copia
   `_save` a `08-global-random`.

Después puedes devolver el ajuste global y el kit a tus valores preferidos y
guardarlos. No hace falta cambiar efectos, CV, grupos de mute ni otros ajustes
para esta primera fase.

## Qué permitirá comprobar

Una investigación pública de otra tarjeta identifica archivos CBOR en
`_save/<kit>.rpl` y `_save/settings.rpl`, con campos `layer_modes`,
`selected_layer` y `layerMode`. Sus correspondencias numéricas y la versión de
firmware de los ejemplos no están completamente verificadas. Aquí se compararán
los bytes y los valores decodificados; no se supondrán offsets fijos ni que la
numeración de los mensajes MIDI coincide con la guardada en la SD.

Antes de habilitar escritura habrá que comprobar también que un archivo generado
por la app se carga correctamente en el módulo, conservando todos los ajustes
ajenos al cambio. El BPM, los pasos y sus velocidades pertenecen al secuenciador
de la app; no se tratarán como parámetros de secuenciador del Rample.

Cuando termines, basta con indicar la ruta local de `Rample SD captures` y la
versión de firmware. No necesitas subir los WAV a GitHub.

Fuentes: [manual oficial de Squarp](https://squarp.net/rample/manual/) e
[investigación pública de Romper](https://github.com/peteb4ker/romper/blob/main/docs/developer/rample-save-integration.md).
