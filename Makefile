INPUT_DIR ?= inputs
OUTPUT_DIR ?= voxels

VOXEL_X ?= 0.4
VOXEL_Y ?= 0.4
VOXEL_Z ?= 0.4

SIZE_X ?= 256
SIZE_Y ?= 256
SIZE_Z ?= 256
PADDING_VOXELS ?= 3
FIELD ?= 1
FIELD_METHOD ?= trapezoid
FIELD_RATE_X ?= 3.7
FIELD_RATE_Y ?= 3.7
FIELD_RATE_Z ?= 1
KERNEL ?=
MAX_UNREACHED_BELOW ?= 5
UNREACHED_CONE_ANGLE ?= 45
ISO_SPACING ?= 1.0
GCODE ?= 1
WALL_COUNT ?= 2
EXTRUSION_WIDTH ?= 0.4
FILAMENT_DIAMETER ?= 1.75
INFILL_SPACING ?= 4

VOXEL_GEN := field-gen/target/release/field-gen
FIELD_GEN_SOURCES := $(shell find field-gen/crates -type f 2>/dev/null)
INPUT_STEMS := $(patsubst $(INPUT_DIR)/%.stl,%,$(wildcard $(INPUT_DIR)/*.stl)) \
	$(patsubst $(INPUT_DIR)/%.STL,%,$(wildcard $(INPUT_DIR)/*.STL))
OUTPUT_SUFFIXES := .occ .bmp .json
OUTPUT_TARGET_PATTERN := $(OUTPUT_DIR)/%.occ $(OUTPUT_DIR)/%.bmp $(OUTPUT_DIR)/%.json
ifneq ($(filter-out 0,$(FIELD) $(GCODE)),)
OUTPUT_SUFFIXES += .ply -clipped.ply
OUTPUT_TARGET_PATTERN += $(OUTPUT_DIR)/%.ply $(OUTPUT_DIR)/%-clipped.ply
endif
ifneq ($(GCODE),0)
OUTPUT_SUFFIXES += .gcode
OUTPUT_TARGET_PATTERN += $(OUTPUT_DIR)/%.gcode
endif
VOXEL_OUTPUTS := $(foreach stem,$(INPUT_STEMS),$(addprefix $(OUTPUT_DIR)/$(stem),$(OUTPUT_SUFFIXES)))

FIELD_ARGS :=
ifneq ($(FIELD),0)
ifneq ($(KERNEL),)
FIELD_ARGS := --kernel $(KERNEL) --max-unreached-below $(MAX_UNREACHED_BELOW) --unreached-cone-angle $(UNREACHED_CONE_ANGLE)
else ifeq ($(FIELD_METHOD),anisotropic)
FIELD_ARGS := --field --field-method $(FIELD_METHOD) --field-rate $(FIELD_RATE_X) $(FIELD_RATE_Y) $(FIELD_RATE_Z) --max-unreached-below $(MAX_UNREACHED_BELOW) --unreached-cone-angle $(UNREACHED_CONE_ANGLE)
else
FIELD_ARGS := --field --field-method $(FIELD_METHOD) --max-unreached-below $(MAX_UNREACHED_BELOW) --unreached-cone-angle $(UNREACHED_CONE_ANGLE)
endif
endif
GCODE_ARGS :=
ifneq ($(GCODE),0)
GCODE_ARGS := --gcode --wall-count $(WALL_COUNT) --extrusion-width $(EXTRUSION_WIDTH) --filament-diameter $(FILAMENT_DIAMETER) --infill-spacing $(INFILL_SPACING)
endif

.PHONY: all voxels clean list-inputs

all: voxels

$(VOXEL_GEN): field-gen/Cargo.toml $(FIELD_GEN_SOURCES)
	cargo build --release --manifest-path field-gen/Cargo.toml

voxels: $(VOXEL_OUTPUTS)
	@if [ -z "$(strip $(VOXEL_OUTPUTS))" ]; then \
		echo "No STL files found in $(INPUT_DIR)"; \
	fi

$(OUTPUT_TARGET_PATTERN) &: $(INPUT_DIR)/%.stl $(VOXEL_GEN) Makefile
	@mkdir -p "$(OUTPUT_DIR)"
	@echo "Voxelizing $< -> $(OUTPUT_DIR)/$*.occ and $(OUTPUT_DIR)/$*.bmp"
	"$(VOXEL_GEN)" "$<" \
		--voxel "$(VOXEL_X)" "$(VOXEL_Y)" "$(VOXEL_Z)" \
		--size "$(SIZE_X)" "$(SIZE_Y)" "$(SIZE_Z)" \
		--padding-voxels "$(PADDING_VOXELS)" \
		--iso-spacing "$(ISO_SPACING)" \
		$(FIELD_ARGS) \
		$(GCODE_ARGS) \
		--output "$(OUTPUT_DIR)/$*"

$(OUTPUT_TARGET_PATTERN) &: $(INPUT_DIR)/%.STL $(VOXEL_GEN) Makefile
	@mkdir -p "$(OUTPUT_DIR)"
	@echo "Voxelizing $< -> $(OUTPUT_DIR)/$*.occ and $(OUTPUT_DIR)/$*.bmp"
	"$(VOXEL_GEN)" "$<" \
		--voxel "$(VOXEL_X)" "$(VOXEL_Y)" "$(VOXEL_Z)" \
		--size "$(SIZE_X)" "$(SIZE_Y)" "$(SIZE_Z)" \
		--padding-voxels "$(PADDING_VOXELS)" \
		--iso-spacing "$(ISO_SPACING)" \
		$(FIELD_ARGS) \
		$(GCODE_ARGS) \
		--output "$(OUTPUT_DIR)/$*"

list-inputs:
	@find "$(INPUT_DIR)" -maxdepth 1 \( -name '*.stl' -o -name '*.STL' \) -print 2>/dev/null || true

clean:
	rm -rf "$(OUTPUT_DIR)"
