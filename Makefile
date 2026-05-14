INPUT_DIR ?= inputs
OUTPUT_DIR ?= outputs
CONFIG ?= inputs/snapmaker-a350.json

VOXEL_GEN := field-gen/target/release/field-gen
FIELD_GEN_SOURCES := $(shell find field-gen/crates -type f 2>/dev/null)
INPUT_STEMS := $(patsubst $(INPUT_DIR)/%.stl,%,$(wildcard $(INPUT_DIR)/*.stl)) \
	$(patsubst $(INPUT_DIR)/%.STL,%,$(wildcard $(INPUT_DIR)/*.STL))
OUTPUT_SUFFIXES := .occ .bmp .json .gcode
OUTPUT_TARGET_PATTERN := $(OUTPUT_DIR)/%.occ $(OUTPUT_DIR)/%.bmp $(OUTPUT_DIR)/%.json
OUTPUT_TARGET_PATTERN += $(OUTPUT_DIR)/%.gcode
VOXEL_OUTPUTS := $(foreach stem,$(INPUT_STEMS),$(addprefix $(OUTPUT_DIR)/$(stem),$(OUTPUT_SUFFIXES)))

.PHONY: all voxels clean list-inputs

all: voxels

$(VOXEL_GEN): field-gen/Cargo.toml $(FIELD_GEN_SOURCES)
	cargo build --release --manifest-path field-gen/Cargo.toml

voxels: $(VOXEL_OUTPUTS)
	@if [ -z "$(strip $(VOXEL_OUTPUTS))" ]; then \
		echo "No STL files found in $(INPUT_DIR)"; \
	fi

$(OUTPUT_TARGET_PATTERN) &: $(INPUT_DIR)/%.stl $(VOXEL_GEN) $(CONFIG) Makefile
	@mkdir -p "$(OUTPUT_DIR)"
	@echo "Voxelizing $< -> $(OUTPUT_DIR)/$*.occ and $(OUTPUT_DIR)/$*.bmp"
	"$(VOXEL_GEN)" "$<" \
		--config "$(CONFIG)" \
		--output "$(OUTPUT_DIR)/$*"

$(OUTPUT_TARGET_PATTERN) &: $(INPUT_DIR)/%.STL $(VOXEL_GEN) $(CONFIG) Makefile
	@mkdir -p "$(OUTPUT_DIR)"
	@echo "Voxelizing $< -> $(OUTPUT_DIR)/$*.occ and $(OUTPUT_DIR)/$*.bmp"
	"$(VOXEL_GEN)" "$<" \
		--config "$(CONFIG)" \
		--output "$(OUTPUT_DIR)/$*"

list-inputs:
	@find "$(INPUT_DIR)" -maxdepth 1 \( -name '*.stl' -o -name '*.STL' \) -print 2>/dev/null || true

clean:
	rm -rf "$(OUTPUT_DIR)"
