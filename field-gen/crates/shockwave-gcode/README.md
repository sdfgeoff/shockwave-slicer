# shockwave-gcode

Marlin-flavoured G-code emission.

This crate owns converting prepared toolpaths into printer commands, including the controlled start sequence, extrusion calculation, temperatures, fan control, and coordinate offsets. It should not decide how geometry is sliced or how paths are generated.
