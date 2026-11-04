# Rample Preview

Domain vocabulary for the shared sample Library and Rample Patch preparation.

## Language

**Library**:
The application's shared collection of Samples and their descriptive names and
user-defined tags. It preserves original audio independently of import sources
and of the requirements of any playback device.
_Avoid_: SD card, Patch

**Sample**:
A reusable sound in the Library, with a descriptive name and user-defined tags.
Its sound is defined by an Original and editable Sample Edits; any number of
Layers can refer to the same Sample.
_Avoid_: Layer, filename

**Original**:
The imported audio file preserved exactly, including its channels and file
metadata, independently of subsequent Sample Edits or device conversions.

**Sample Edits**:
The saved, reversible processing settings that define a Sample's current sound
from its Original, including trim, gain, pitch, equalization, echo and reverb.

**Sample Revision**:
A saved state of a Sample's edits that can reproduce an earlier version of its
sound, including a version previously exported from the application.

**Variant**:
An independently editable Sample derived from another Sample, sharing its
Original while retaining its own name, tags and Sample Edits.

**Trash**:
The recoverable collection of retired Samples hidden from the add-Sample picker
but still available to Layers that already reference them.

**Patch**:
A named, complete Rample SD musical configuration containing Banks, Kits and their
settings. Multiple Patches use the same Library.
_Avoid_: Kit, exact SD image

**Bank**:
A lettered group of Kits, from A to Z, within a Patch.

**Kit**:
A group of four Voices and their playback settings, occupying a bank letter and
kit number within a Patch.
_Avoid_: Patch

**Voice**:
One of the four playback channels in a Kit, with its own ordered Layers and
playback settings.

**Layer**:
An occurrence of a Sample at a particular position in a Voice's ordered list.
The same Sample may occupy multiple Layers, including within the same Voice.
_Avoid_: Sample when referring to a position

**Import**:
An intake of sounds into the Library and, for a Rample SD source, its musical
configuration into a Patch. A completed Import has no continuing dependency on its source.

**Build Patch**:
The generation of a Rample-compatible directory from a Patch: its Bank/Kit
folders and ordered, prepared audio files. It never writes native settings
(`_save`); those stay on the device.
