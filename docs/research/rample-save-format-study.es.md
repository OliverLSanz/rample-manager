# Estudio: qué falta para controlar al 100 % el formato `_save` del Rample

Fecha: 2026-11-05. Equipo de referencia: Rample Turbo, firmware 2.00, kit `Z0`.

> **Cambio de alcance (2026-11-05, [ADR 0007](../adr/0007-read-native-settings-and-never-write-save.md)).**
> La app gestiona solo los ficheros de audio y **nunca escribe `_save`**. Lo que
> sigue sobre escritura (lote F, plantilla A7–A8, sección 4) queda como
> referencia histórica y ya no bloquea el #13. Para lectura siguen siendo útiles:
>
> - A1–A3: valores globales 2–4.
> - El resto del lote A: comprobar las voces 2–4.
> - Lote G: orden de capas, para leer bien `selected_layer`.
> - Lotes B–E: solo si algún día se quieren mostrar o simular esos parámetros
>   en la vista previa.
Este documento continúa la
[investigación pública previa](rample-kit-tools-and-native-settings.md) (CBOR,
fixtures de Romper, herramientas), el
[protocolo de captura ya seguido](../rample-sd-capture.es.md) y el
[ADR 0003](../adr/0003-require-native-settings-for-complete-patch-builds.md).
No repite esa base; se centra en lo que falta.

Las capturas de Oliver (`Rample SD Captures/`, 01–08, 2026-11-04) son privadas
y no se suben al repositorio. Para este estudio se decodificaron otra vez con un
lector CBOR mínimo y desechable escrito para la ocasión. No se usó código de terceros.

## Leyenda de estados

| Estado | Significado |
| --- | --- |
| **Verificado** | Demostrado con un diff controlado de una sola modificación en el Rample de Oliver (FW 2.00). |
| **Verificado (bytes)** | Clave, tipo y valor por defecto observados en los bytes, sin un cambio controlado que confirme qué control representan. |
| **Hipótesis** | Deducción razonable del manual, el changelog o los valores observados. No basta para escribir. |
| **Desconocido** | No hay datos. |

## 0. Conclusiones rápidas

- El contenedor está resuelto para FW 2.00. Cada archivo no vacío es un único
  mapa o array CBOR de longitud definida, con enteros en forma mínima y claves
  ordenadas por bytes en todos los niveles. No hay bytes sobrantes.
- Ya son **Verificados** `layer_modes` (por kit), `selected_layer` (por kit) y
  los valores 0 y 1 de `layerMode` (global). En total son 3 claves de las 32 conocidas.
- Para escribir solo modos de capa y capa seleccionada (#13) faltan unas 21
  capturas o pruebas: lotes A, G y F obligatorios. Para el control total faltan
  unas 100 más. El lote H y parte de A y F se repiten por cada firmware soportado.
- Modificar `layer_modes` (0–4) o `selected_layer` (0–11) **nunca cambia la
  longitud del archivo**. CBOR codifica 0–23 en el byte inicial, así que el
  cambio es un byte in situ y el resto queda idéntico.
- El ejemplo `Z1.rpl` está «sin tocar», pero tiene `layer_modes=[0,0,0,0]`
  (MANUAL) mientras el global era RANDOM (1). Por tanto, el contenido de un
  `.rpl` por defecto **no** equivale al estado de un kit nunca guardado con
  STORE. Hay que averiguar cómo nació `Z1.rpl` (prueba A7–A8).

## 1. Inventario

### 1.1 Archivos de `_save`

| Archivo | Lo escribe | Contenido observado (FW 2.00) | Significado | Estado |
| --- | --- | --- | --- | --- |
| `<kit>.rpl`, p. ej. `Z0.rpl` | STORE (manual: «Save current kit parameters & assignments») | Mapa de 13 claves, 277 B | Parámetros, asignaciones, mute groups y capas del kit | Clave y forma verificadas (bytes). Semántica: ver 1.2 |
| `settings.rpl` | SAVE SETTINGS | Mapa de 17 claves, 240 B | Ajustes globales | Ver 1.5 |
| `global_assign.rpl` | Desconocido: ¿SAVE SETTINGS, o asignar con ASSIGN=GLOBAL? | Array de 4 `{param,voice}`, 61 B | Asignaciones CV cuando ASSIGN=GLOBAL | Verificado (bytes). Semántica: hipótesis |
| `autosave_<kit>.rpl` | Firmware, con AUTOSAVE=ON | 0 B; `autosave_Z0.rpl` mientras se usaba Z0 | Marcador del último kit, cargado al arrancar | Hipótesis. Romper cita un foro según el cual se escribe tras un rato en el mismo kit |

Lo que **no** está en ningún archivo de FW 2.00 es lo siguiente. No hay nombres
de archivo, número de capas, mono/estéreo, BPM ni pasos. SLICER es global y no
existe por kit. Tampoco hay versión de firmware ni de formato.

### 1.2 Mapa de kit `<kit>.rpl` (FW 2.00)

Todos los arrays tienen 4 elementos, uno por voz. Que el índice 0 sea SP1 está
verificado. Que los índices 1–3 sean SP2–SP4 es una hipótesis muy probable que
el lote A confirma. La sección del manual es la de
[Audio effects & parameters](https://squarp.net/rample/manual/).

| Clave | Control (manual) | Por defecto observado | Rango hipotético | Estado |
| --- | --- | --- | --- | --- |
| `layer_modes` | Modo de capa por voz (ASSIGN → ①–④ → ①–④) | `[0,0,0,0]` | 0 MANUAL, 1 RANDOM, 2 CYCLIC, 3 REVERSE CYCLIC, 4 VELOCITY | **Verificado** con capturas 01–05 (voz 1) |
| `selected_layer` | Capa MANUAL elegida con el encoder | `[0,0,0,0]` | 0–11, empieza en cero (capa 4 → 3) | **Verificado** en la voz 1 (captura 06). Se desconoce si STORE guarda la posición en curso en CYCLIC |
| `pitch` | Pitch ①, de −1 a +1 octava | 127 | 0–254, con 127 sin transposición | Hipótesis. Falta saber si guarda la posición cruda o la cuantizada con PITCH=CHROMATIC |
| `bitcrush` | Bits ②, dos algoritmos (a la izquierda y a la derecha según 3.00) | 127 | 0–254, con 127 apagado | Hipótesis. La semántica cambió en 2.00 y en 3.00 |
| `filter` | Filtro ③, LP a la izquierda y HP a la derecha | 127 | 0–254, con 127 neutro | Hipótesis. Romper vio 189 |
| `freeze` | Freeze ④, «off» en el punto medio; algo 1 a la izquierda y algo 2 a la derecha | 127 | 0–254, con 127 apagado | Hipótesis. Cambió en 2.00 y en 3.00 |
| `level` | Levels/Drive (ASSIGN+LISTEN) | 127 | 0–254. El manual es ambiguo: «0–50 % clean gain, 50–100 % drive» | Hipótesis. Valores dB desde 2.00. Romper vio 42 |
| `start` | Start point (doble toque) | 0 | 0–254, con 0 al inicio del sample | Hipótesis. Su relación con SLICER (EXP o /N) es desconocida |
| `length` | Length (doble toque); el manual dice «de 100 % a 0 %» | 254 | 0–254, con 254 duración completa | Hipótesis. La dirección del mando es ambigua |
| `env` | Env: a la izquierda ataque, a la derecha decay | 127 | 0–254, con 127 neutro | Hipótesis |
| `loop` | Run mode: one shot, toggle loop o gate high only | 127 | Tres zonas de un mando o tres valores (¿0, 127, 254?) | Hipótesis. Romper vio 0 |
| `assignments` | ASSIGN por kit, CV1–CV4 | 4 × `{param:9, voice:0}` | Ver 1.3 | Verificado (bytes) |
| `mute_group` | Mute groups | Matriz 4×4 de `false` | Ver 1.4 | Verificado (bytes), solo la forma |

El patrón de 127 como centro y 254 como extremo apunta a una escala 0–254, quizá
2 × el valor MIDI 0–127. Sin embargo, el changelog 1.3 dice «MIDI chart center
is now 63 or 64». La relación MIDI↔SD **no** está verificada. No se debe usar la
tabla MIDI como enum de la SD ([firmware](https://squarp.net/rample/firmware/)).

### 1.3 `assignments` (kit) y `global_assign.rpl` (global)

| Campo | Manual («Assign a CV input») | Observado | Hipótesis | Estado |
| --- | --- | --- | --- | --- |
| Índice del array | CV 1–4 | 4 elementos | Índice 0 = CV1 | Hipótesis |
| `voice` | SP1, SP2, SP3, SP4, All Samples | Kit: 0,0,0,0. Global: 0,1,2,3 | 0–3 = SP1–SP4; «All» tal vez 4 | Hipótesis |
| `param` | Por orden en la UI: Pitch, Bits, Filter, Freeze, Start, Length, Env, Run, Levels/drive, Layers, T–F, V/oct | 9 en todo | **Ambiguo.** En base 0 por orden de la UI, 9 = Layers, lo que no parece un valor por defecto creíble. Otra hipótesis: 9 = «sin asignar», si los 9 parámetros originales eran 0–8 y Layers, T–F (1.1) y V/oct (1.4) se añadieron después | Desconocido |

Los valores por defecto difieren: el kit usa `voice:0` ×4 y el global usa
`voice:0..3`. Esto sugiere una inicialización distinta y no una asignación
activa. El changelog 1.4 indica que los parámetros vuelven a su valor por
defecto al borrar o modificar una asignación, así que en los diffs del lote C
pueden cambiar otras claves.

### 1.4 `mute_group`

El manual dice: «each sample can be muted by up to 3 other samples». En la UI se
mantiene ASSIGN, se elige primero la voz silenciada y luego las que silencian.

| Pregunta | Hipótesis | Estado |
| --- | --- | --- |
| Orientación | `mute_group[silenciada][disparadora]`, siguiendo el orden de la UI | Desconocido |
| Diagonal (una voz se silencia a sí misma) | Siempre `false` | Desconocido |
| ¿Se ha visto `true`? | No | Desconocido |

### 1.5 `settings.rpl` (FW 2.00, 17 claves)

Las secciones del manual citadas son Settings, «MIDI Keyboard Split» y Advanced parameters.

| Clave | Ajuste del manual | Observado (Oliver) | Romper | Rango o enum hipotético | Estado |
| --- | --- | --- | --- | --- | --- |
| `layerMode` | LAYER (por defecto de los kits nunca guardados con STORE) | 1 | 1 | 0 MANUAL, 1 RANDOM, 2 CYCLIC, 3 REVERSE, 4 VELOCITY | **0 y 1 verificados** (07/08); 2–4 son hipótesis |
| `anti_clic` | ANTICLIC | 1 | 1 | 0 OFF, 1 ON | Hipótesis |
| `assign` | ASSIGN KIT/GLOBAL | 0 | 0 | 0 KIT, 1 GLOBAL | Hipótesis |
| `autosave` | AUTOSAVE | 1 | 1 | 0 OFF, 1 ON | Hipótesis (es coherente con que exista `autosave_Z0.rpl`) |
| `cv_in_range` | CV IN: 0–5 V o −5–5 V | 1 | 1 | 2 valores; no se sabe cuál es 1 | Desconocido |
| `flip` | FLIP | 0 | 0 | 0 o 1 | Hipótesis |
| `midi_channel_in` | CHANNEL | 1 | 8 | 1–16 o 0–15; ¿existe OMNI? | Desconocido |
| `midi_velocity` | VELOCITY: LEVEL, PITCH, BIT, FILTER (¿y OFF?) | 0 | 0 | Índice en la lista | Desconocido |
| `note_sp1`…`note_sp4` | SP1–SP4: nota 0–127, «Chromatic split» (9 rangos) o «Layer split» (10 rangos) | **128, 130, 132, 134** | 48–51 | 0–127 es una nota. Por encima de 127 es probablemente un índice de rango del menú, quizá 128 + n. Ningún diff lo demuestra todavía | Desconocido. Antes de capturar, mira qué muestra la pantalla |
| `pitch_tracking_chromatic` | PITCH (cuantización cromática) | 0 | 0 | 0 o 1 | Hipótesis |
| `receive_pitchbend` | BEND: ¿OFF?, ALL/SP1–4 × LEVEL/PITCH/BITS/FILTER/FREEZE | 0 | 0 | Índice entre unas 26 opciones | Desconocido |
| `receive_progchange` | PROGRAM CHANGE | 1 | 1 | 0 o 1 | Hipótesis |
| `slicer_quantize_postv200` | SLICER: EXP, /8, /16, /32, /64, /128, /12, /24, /48 | 2 | 2 | Índice. El orden del dispositivo puede no ser el del manual | Desconocido |
| `vu_meters` | VUMETER | 1 | 1 | Índice. La lista cambia según el firmware (TIMELINE, EXPLOD e INPUTS antes; PRO y PEAK en 3.00) | Desconocido |

Entre las capturas 07 y 08 el ajuste volvió a RANDOM, y `settings.rpl` quedó
idéntico byte a byte al de 01. Es una buena señal de serialización
determinista.

### 1.6 Se guarda según el manual, pero aún no se ha visto

| Elemento | Fuente | Dónde se guardaría | Estado |
| --- | --- | --- | --- |
| TAPE (0–127), maestro | Manual («saved as part of your settings»), FW 3.00 | `settings.rpl`, clave desconocida | Desconocido. No existe en 2.00 |
| COMPRESS (9 presets), % COMPRESS (0–200 %), SIDECHAIN (SP1–4 u off) | Manual (Compressor effect), FW 3.00 | `settings.rpl` | Desconocido |
| PUNCH (FX o VOICE FX; ¿off?) | Manual (Punch effects), FW 3.00 | Probablemente `settings.rpl` | Desconocido. El manual no dice si se guarda |
| Nuevas semánticas de Bitcrusher, Freeze y Drive | Changelog 3.00 | Mismas claves con otro significado, o claves nuevas | Desconocido |
| Opciones nuevas de VU (PRO, PEAK) | Changelog 3.00 | `vu_meters` | Hipótesis |
| COPY/PASTE, momentary, punch en vivo, «hold ASSIGN keeps FX» | Manual | Probablemente solo en RAM | Hipótesis |
| Escenas, patrones o secuencias | No existen en el manual | Ninguno | El manual no describe nada |

La revisión del manual disponible describe funciones de 3.00. Oliver está en
2.00, así que hay menús del manual que su equipo no tiene.

## 2. Incógnitas del contenedor y del comportamiento

La columna «¿Importa para #13?» supone que #13 solo cambia `layer_modes` y
`selected_layer`, y que crea `<kit>.rpl` cuando falte.

| Incógnita | Por qué importa | ¿Importa para #13? | Prueba |
| --- | --- | --- | --- |
| Firmware 2.00 o 3.00 | Las claves y la semántica cambian. Los archivos no llevan versión. El 3.00 corrige que «el kit seleccionado cambia al azar tras SAVE o STORE» | **Sí**: decidir antes de nada y fijar el perfil | Lote H |
| ¿Acepta el dispositivo un `.rpl` escrito por el ordenador? | Es la base de todo | **Sí** | F1–F3 |
| Kit nunca guardado: ¿qué escribe su primer STORE? ¿Cargar un kit crea archivo? | Plantilla para crear archivos; origen de `Z1.rpl` | **Sí** | A7–A8, F4 |
| Borrar `<kit>.rpl` | Staff de Squarp: «resetea el kit» ([foro](https://squarp.community/t/folder-settings-mishap/12026)) | **Sí** (camino de recuperación) | F6 |
| `selected_layer` ≥ número de capas | Si cambia el número de capas, el índice puede quedar fuera de rango | **Sí** (definir el recorte) | F8 |
| Orden de las capas (natural o ASCII, mayúsculas, puntuación, acentos, LFN o 8.3, orden de directorio) | `selected_layer` es un índice sobre ese orden. Hay que conservar los nombres de usuario (AGENTS.md) | **Sí** | Lote G |
| Más de 12 capas, o capas inválidas intercaladas | ¿Cambia el índice? | Sí, en parte | G4 |
| ¿STORE en CYCLIC o RANDOM guarda la capa en curso en `selected_layer`? | Interpretar la lectura | Recomendado | A6 |
| Archivos ajenos en `_save` (`._Z0.rpl` de AppleDouble, `.DS_Store`, temporales) | macOS los crea en FAT o exFAT; ocurre con escrituras atómicas | **Sí** | F7 |
| ¿Lee el arranque con AUTOSAVE el `.rpl` del disco? | El kit del marcador se carga al encender | **Sí** | F5 |
| Claves ausentes, claves extra, orden de claves, enteros no mínimos, arrays de otra longitud, valores fuera de rango | Robustez y compatibilidad futura. La política es no producirlos nunca | No, si se aplica la política de la sección 4 | F9–F14 (opcionales) |
| ¿Conserva STORE las claves desconocidas, o reescribe desde su estructura en RAM? | Saber si las claves de 3.00 sobreviven en 2.00 | No | F11 |
| Mayúsculas del nombre (`z0.rpl`) y carpetas de kit en minúscula | FAT no distingue mayúsculas; el firmware quizá sí (issue 573 de Romper) | No (escribir siempre en mayúsculas, como el dispositivo) | F15 opcional |
| Semántica del marcador `autosave_` (cuándo se crea o se renombra; kit inexistente) | Build Patch puede mover o borrar kits | Preservarlo basta | E-autosave |
| Mtime de 0 en archivos del dispositivo (observado por Romper) | Nuestros mtimes serán reales | No, salvo que F1 falle | F1 |

## 3. Plan de capturas para Oliver

### 3.1 Preparación común (añadir a la guía existente)

1. **Decide primero el firmware.** Si piensas actualizar a 3.00, hazlo **antes**
   de los lotes B–F y ejecuta el lote H. Todo lo verificado vale para un único
   firmware.
2. Sigue el procedimiento de la [guía](../rample-sd-capture.es.md): módulo
   apagado, copia completa de `_save` en una carpeta nueva por captura y nunca
   sobrescribir. Usa los mismos nombres de carpeta de la tabla, con el prefijo
   numérico.
3. **Un único cambio intencionado por captura.** Tras cada STORE o SAVE,
   comprueba que el kit mostrado sigue siendo el correcto (fallo conocido de
   2.00).
4. En `notes.txt` apunta firmware, kit, lo que mostraba la pantalla y lo que
   oíste. En el lote G, apunta el número de pitidos por posición.
5. Para volver a la base entre capturas de mandos, usa **Settings → RESET +
   STORE**. B0 comprueba que RESET deja el archivo igual que `Z1.rpl`.
6. **Pregunta previa, sin captura:** ¿hiciste alguna vez STORE en Z1? ¿Qué
   muestra ahora la pantalla en Settings → SP1, SP2, SP3 y SP4 (los valores 128,
   130, 132 y 134)? ¿Qué kits de la SD no tienen `.rpl`?

### 3.2 Lotes, por orden recomendado de ejecución

La numeración de carpetas sigue el orden de ejecución y lleva la letra del lote.
En los lotes C, D, E y H continúa de forma correlativa, por ejemplo
`60-C1-cv1-pitch`, `61-C2-cv1-bits` o `85-D1-sp1-por-sp2`.

En la columna #13, **Sí** significa obligatorio para #13, y **100 %** significa
que solo hace falta para el control total.

#### Lote A — modos y capa restantes (kit Z0; 8 capturas, todas para #13)

| Carpeta | Acción | Verifica | #13 |
| --- | --- | --- | --- |
| `09-A1-global-cyclic` | Settings → LAYER → CYCLIC, SAVE SETTINGS | `layerMode=2` | Sí |
| `10-A2-global-reverse` | LAYER → REVERSE CYCLIC, SAVE SETTINGS | `layerMode=3` | Sí |
| `11-A3-global-velocity` | LAYER → VELOCITY, SAVE SETTINGS. Deja VELOCITY puesto para A7 | `layerMode=4` | Sí |
| `12-A4-voz4-velocity` | Voz 4 → VELOCITY, STORE | Índice 3 = SP4 | Sí |
| `13-A5-voz2-capa2` | Voz 2 en MANUAL (necesita 2 o más capas) → capa 2, STORE | `selected_layer[1]=1` | Sí |
| `14-A6-cyclic-en-curso` | Voz 1 → CYCLIC, dispárala 2 veces (capa en curso 3), STORE | ¿Cambia `selected_layer` con la posición en curso? | Sí |
| `15-A7-kit-nuevo-cargado` | Con el global en VELOCITY, carga un kit sin `.rpl` (anótalo) y no guardes. Copia `_save` | ¿Cargar crea archivo? Anota el modo que muestra cada voz | Sí |
| `16-A8-kit-nuevo-store` | En ese mismo kit, STORE sin cambios | Plantilla real de un kit nunca guardado. ¿`layer_modes=[4,4,4,4]`? | Sí |

#### Lote G — orden de capas (unos 5 kits de prueba; obligatorio para #13)

Usa kits libres, por ejemplo `Y0`–`Y4`, con WAV ficticios mono, 16 bit,
44,1 kHz, de 0,5 s o más. Cada archivo contiene **N pitidos**, donde N es un
identificador que no coincide con el orden esperado. Nosotros prepararemos los
WAV y una tabla nombre→N. **Cópialos a la SD uno a uno y en orden inverso** al
esperado, por ejemplo con `cp` en Terminal en el orden indicado. Así se
distingue «ordenado» de «orden del directorio». En MANUAL, recorre las capas con
el encoder, pulsa para escuchar y apunta «posición → N pitidos».
Pista sin verificar: al pulsar ① durante la selección de kit se ve el nombre del
sample, quizá el de la primera capa.

| Carpeta | Kit y nombres (voz 1) | Pregunta |
| --- | --- | --- |
| `17-G1-numeros` | `1.wav`, `1-1`, `1-2`, `1-9`, `1-10`, `1-02`, `1-010`, `1-100`, `10.wav` | ¿Orden natural o ASCII? ¿Ceros a la izquierda? ¿`10.wav` es voz 1? |
| `18-G2-mayus-puntuacion` | `1apple`, `1Banana`, `1cherry`, `1Date`, `1 x`, `1-x`, `1_x`, `1.x`, `1(x`, `1~x` | ¿Distingue mayúsculas? ¿Cómo ordena los signos (ASCII: espacio ( - . _ letras ~)? |
| `19-G3-unicode-lfn` | `1e`, `1é`, `1f`, `1n`, `1Ñ`, `1o`, `1kick.WAV`. Crea `1-nombre-largo-b.wav` **antes** que `1-nombre-largo-a.wav` | Acentos; extensión en mayúsculas; ¿ordena por nombre largo o por el corto 8.3 (`~1`, `~2`)? |
| `20-G4-13-capas` | `1-01`…`1-13`, más un `._1-05.wav` de AppleDouble | ¿Qué capa se descarta? ¿Se ignora el archivo `._`? |
| `21-G5-store-indice` | En G1, MANUAL en la posición 5 y STORE; copia `_save` | El índice guardado coincide con el orden observado |

Son 5 pruebas. Solo G5 requiere copiar `_save`; las demás son tablas de escucha.

#### Lote F — escritura de ida y vuelta con archivos generados por la app (obligatorio en parte)

Nosotros entregaremos cada archivo con su SHA-256. Tú lo copias a `_save` con
el módulo apagado, compruebas en pantalla y de oído, haces **STORE sin cambios**
y copias `_save` a la carpeta. Lo esperado es que el archivo re-almacenado sea
idéntico al entregado. Haz siempre antes un respaldo completo de la SD.

| Carpeta | Archivo entregado | Comprobar | #13 |
| --- | --- | --- | --- |
| `22-F1-identico` | `Z0.rpl` con los mismos bytes, escrito por la app | Carga normal; descarta problemas de mtime y escritura | Sí |
| `23-F2-modos` | `Z0.rpl` con `layer_modes=[2,1,3,4]` | Cada voz muestra su modo; STORE da los mismos bytes | Sí |
| `24-F3-capa` | `layer_modes[0]=0`, `selected_layer[0]=2` | Suena la capa 3 | Sí |
| `25-F4-kit-creado` | `Y5.rpl` nuevo (kit nunca guardado), creado con la plantilla de A8 y modos `[1,2,3,4]` | Carga los modos; STORE da los mismos bytes | Sí |
| `26-F5-arranque` | Como F2, sobre el kit del marcador `autosave_` | Tras encender se aplican los modos nuevos | Sí |
| `27-F6-borrado` | Borra `Z0.rpl` | Las voces vuelven a LAYER global y los mandos a sus valores por defecto; STORE lo recrea igual que en A8 | Sí |
| `28-F7-ajenos` | Añade `._Z0.rpl` y `.DS_Store` a `_save` | Sin efecto; ¿los borra el dispositivo? | Sí |
| `29-F8-capa-fuera` | `selected_layer[0]=11` con 4 capas | ¿Recorta, da la vuelta, queda en silencio o se cuelga? | Sí (define el recorte) |
| `30-F9-modo-fuera` | `layer_modes[0]=5` | Robustez | 100 % |
| `31-F10-clave-ausente` | Sin `freeze` | ¿Carga? ¿Qué valor usa? ¿STORE la repone? | 100 % |
| `32-F11-clave-extra` | `zz_test: 1` | ¿Carga? ¿STORE la conserva? | 100 % |
| `33-F12-entero-largo` | 127 codificado como `19 00 7F` | ¿Lo admite? | 100 % |
| `34-F13-orden-claves` | Claves desordenadas | ¿Lo admite? | 100 % |
| `35-F14-array-5` | `pitch` con 5 elementos | ¿Lo admite? | 100 % |
| `36-F15-minusculas` | `z0.rpl` | ¿Lo encuentra? | 100 % |

#### Lote B — escalas de los mandos (kit Z0, voz 1; unas 22 capturas, 100 %)

| Carpetas | Acción | Verifica |
| --- | --- | --- |
| `37-B0-reset` | RESET + STORE | La base es igual a `Z1.rpl` |
| `38`–`53` (`B1-pitch-min`, `B2-pitch-max`, … ) | Para pitch, bits, filter, freeze, level, start, length y env: mando al tope izquierdo, STORE, captura, RESET; luego al tope derecho | Extremos (¿0 y 254?), dirección y si el centro es 127. Son 16 capturas |
| `54-B17-run-oneshot`, `55-B18-run-toggle`, `56-B19-run-gate` | Run mode explícito | Valores de `loop` |
| `57-B20…` (opcional) | Con fuente MIDI: CC del pitch a 32 y a 96 (y algún parámetro más) | Fórmula MIDI→SD (¿2×?) |
| `5x-B2x-slicer` (opcional) | SLICER /16 y start en la primera división | ¿`start` guarda la posición cruda o cuantizada? |

#### Lote C — asignaciones CV (kit Z0; unas 22 capturas, 100 %)

| Carpetas | Acción | Verifica |
| --- | --- | --- |
| C1–C12 | CV1 → SP1 → cada parámetro de la lista (Pitch … V/oct) | Enum completo de `param`, incluido qué significa 9 |
| C13–C16 | CV1 → SP2, SP3, SP4, All Samples (con Pitch) | Enum de `voice` |
| C17–C19 | CV2, CV3, CV4 → SP1 Pitch | Índice del array = CV |
| C20 | Borrar o desasignar CV1 (el manual no dice cómo) | Valor de «sin asignar» |
| C21 | ASSIGN = GLOBAL, SAVE SETTINGS | `assign=1` |
| C22–C23 | En GLOBAL, CV1 → SP2 Filter. Copia tras SAVE SETTINGS y otra vez tras STORE | Qué acción escribe `global_assign.rpl` y si cambia el kit |

#### Lote D — mute groups (4–5 capturas, 100 %)

| Carpeta | Acción |
| --- | --- |
| D1 | SP1 silenciada por SP2 (ASSIGN mantenido, ①, ②) → orientación |
| D2 | Añadir: SP2 silenciada por SP1 + SP4 |
| D3 | SP4 silenciada por SP1 + SP2 + SP3 (máximo) |
| D4 | Borrar el grupo de SP1 (ASSIGN mantenido, ①) |
| D5 (opcional) | Intentar que SP3 se silencie a sí misma (diagonal) |

#### Lote E — ajustes globales (unas 30 capturas en versión reducida, unas 45 completo; 100 %)

Cada captura cambia una sola clave con SAVE SETTINGS y después la devuelve a su
valor.

| Clave | Capturas |
| --- | --- |
| `anti_clic`, `autosave`, `cv_in_range`, `flip`, `pitch_tracking_chromatic`, `receive_progchange` | 1 cada una (6) |
| `midi_channel_in` | Canal 2, canal 16 y OMNI si existe (2–3) |
| `midi_velocity` | Todas las opciones del menú (4–5) |
| `receive_pitchbend` | Primera, segunda, ALL LEVEL, SP1 LEVEL, ALL PITCH y última (unas 6) |
| `note_sp1` | Nota 0, 60 y 127; chromatic primera y última; layers primera y última (7) |
| `slicer_quantize_postv200` | Primera, segunda y última del menú (3); completo, 8 |
| `vu_meters` | Primera, segunda y última (3); completo, unas 10. Apunta los nombres que muestra la pantalla |
| Marcador `autosave_` | Cambiar a Z1, esperar 2 min y apagar; luego con AUTOSAVE=OFF (2) |

#### Lote H — solo si se adopta el FW 3.00 (unas 14 capturas)

1. Copia `_save` justo después de actualizar, sin tocar nada (¿reescribe algo la actualización?).
2. SAVE SETTINGS sin cambios (¿aparecen claves nuevas?).
3. STORE de Z0 sin cambios (¿cambian las claves del kit?).
4. TAPE 0 y 127; COMPRESS con el primer y el último preset; % COMPRESS 0 y 200;
   SIDECHAIN SP1 y off; PUNCH en cada modo; VU PRO.
5. Repetir A1 y A4 y los F1–F3 en 3.00 antes de habilitar la escritura en ese firmware.

### 3.3 Resumen de esfuerzo

| Lote | Capturas o pruebas | Para #13 | Para el 100 % |
| --- | ---: | --- | --- |
| A — modos y capa | 8 | **Sí** | Sí |
| G — orden de capas | 5 | **Sí** | Sí |
| F — ida y vuelta | 8 obligatorias + 7 opcionales | **F1–F8** | Todas |
| B — mandos | ~22 | No | Sí |
| C — asignaciones | ~22 | No (solo preservar) | Sí |
| D — mute | 4–5 | No (solo preservar) | Sí |
| E — ajustes | ~30–45 | No (no se escribe `settings.rpl`) | Sí |
| H — FW 3.00 | ~14 | Solo si el destino es 3.00 | Sí, por firmware |
| **Total** | **unas 21 para #13; unas 120–135 en total** | | |

## 4. Mínimo para desbloquear la escritura y control total

> Fuera de alcance desde el ADR 0007; se conserva como referencia.

### 4.1 Mínimo para #13 (Build Patch escribe solo modos de capa y capa seleccionada)

1. Perfil de firmware decidido y anotado. Hoy es 2.00.
2. Lote A completo: valores 2–4 del global para interpretar kits sin archivo,
   índice de voz confirmado, semántica de CYCLIC en curso y plantilla de kit
   nunca guardado (A8).
3. Lote G: comparador de orden verificado en el hardware para los nombres de
   usuario. Si un nombre cae en un caso no verificado, por ejemplo acentos o una
   colisión 8.3, la app advierte y no escribe `selected_layer`, o bloquea.
4. F1–F8 superados: carga, modos, capa, kit creado, arranque, borrado, archivos
   ajenos y recorte de índice.

### 4.2 Control total

Todo lo anterior, más los lotes B, C, D y E, más F9–F15 para conocer la
tolerancia. Después, el lote H por cada firmware soportado. Solo entonces se
podrán editar mandos, asignaciones, mute groups y ajustes globales, o importar
un `.rpl` como estado musical completo.

### 4.3 Política de escritura recomendada

1. **Leer de forma estricta y acotada.** Un único ítem de longitud definida y
   sin bytes sobrantes, con claves de texto sin duplicados. El archivo debe
   re-codificarse **idéntico byte a byte**; si no, solo se lee.
2. **Comprobar el perfil antes de escribir.** El kit tiene exactamente las 13
   claves conocidas, arrays de 4 y valores en rango. Cualquier diferencia, como
   claves de 3.00, bloquea la escritura con un mensaje claro. Las claves
   desconocidas se preservan, nunca se descartan.
3. **Cambiar lo mínimo.** Solo `layer_modes[i]` (0–4) y `selected_layer[i]`
   (0…n−1 según el orden verificado). Es un cambio de un byte in situ; el resto
   queda idéntico. Luego se decodifica el resultado y se compara campo a campo
   con el original.
4. **Crear archivos solo con la plantilla capturada (A8) del mismo firmware.**
   Nunca se copian valores de otra tarjeta ni de Romper.
5. **No tocar en #13** `settings.rpl`, `global_assign.rpl`, `autosave_*.rpl` ni
   archivos desconocidos.
6. **Conservar los originales** según los ADR
   [0003](../adr/0003-require-native-settings-for-complete-patch-builds.md) y
   [0005](../adr/0005-reconcile-patch-builds-with-recoverable-destination-changes.md):
   copia de `_save` completa con manifiesto SHA-256 antes de escribir; escritura
   en temporal más renombrado atómico en el mismo volumen; re-lectura y
   verificación; sin xattrs, para que no aparezcan `._*`.
7. **El `.rpl` viaja con el kit.** Si un kit cambia de ranura, su archivo
   completo se mueve con él. Si cambian el orden o el número de capas,
   `selected_layer` se recalcula para la misma ocurrencia de Layer.
8. **Escribir solo con el módulo apagado** (el manual prohíbe extraer la SD con
   el módulo encendido). Después, avisar de que un STORE en el dispositivo
   sobrescribirá lo escrito.

## Fuentes

- Manual oficial de Squarp: <https://squarp.net/rample/manual/>, con una copia
  local en PDF A5 en la revisión de 3.00.
- Changelog de firmware: <https://squarp.net/rample/firmware/>, con 3.00
  (2026-02-27), 2.00 (2024-02-15) y anteriores.
- Respuesta del staff de Squarp sobre `_save` y el borrado de `.rpl`:
  <https://squarp.community/t/folder-settings-mishap/12026>.
- Romper, commit fijado `3539120c`, solo como fuente de hipótesis (MIT; no se
  copió código):
  [rample-save-integration.md](https://github.com/peteb4ker/romper/blob/3539120c5f3c120233f34308b30a799ceeb0f9f7/docs/developer/rample-save-integration.md)
  y [shared/rampleSave.ts](https://github.com/peteb4ker/romper/blob/3539120c5f3c120233f34308b30a799ceeb0f9f7/shared/rampleSave.ts).
- Capturas privadas de Oliver, 01–08 (FW 2.00, 2026-11-04). No se suben al repositorio.
