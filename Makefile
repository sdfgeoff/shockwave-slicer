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
FIELD_RATE_X ?= 3.7
FIELD_RATE_Y ?= 3.7
FIELD_RATE_Z ?= 1

VOXEL_GEN := field-gen/target/release/field-gen
FIELD_GEN_SOURCES := $(shell find field-gen/src -type f 2>/dev/null)

.PHONY: all voxels clean list-inputs

all: voxels

$(VOXEL_GEN): field-gen/Cargo.toml $(FIELD_GEN_SOURCES)
	cargo build --release --manifest-path field-gen/Cargo.toml

voxels: $(VOXEL_GEN)
	@mkdir -p "$(OUTPUT_DIR)"
	@set -e; \
	found=0; \
	field_args=""; \
	if [ "$(FIELD)" != "0" ]; then \
		field_args="--field --field-rate $(FIELD_RATE_X) $(FIELD_RATE_Y) $(FIELD_RATE_Z)"; \
	fi; \
	for stl in "$(INPUT_DIR)"/*.stl "$(INPUT_DIR)"/*.STL; do \
		[ -e "$$stl" ] || continue; \
		found=1; \
		base=$$(basename "$$stl"); \
		name=$${base%.*}; \
		echo "Voxelizing $$stl -> $(OUTPUT_DIR)/$$name.occ and $(OUTPUT_DIR)/$$name.bmp"; \
		"$(VOXEL_GEN)" "$$stl" \
			--voxel "$(VOXEL_X)" "$(VOXEL_Y)" "$(VOXEL_Z)" \
			--size "$(SIZE_X)" "$(SIZE_Y)" "$(SIZE_Z)" \
			--padding-voxels "$(PADDING_VOXELS)" \
			$$field_args \
			--output "$(OUTPUT_DIR)/$$name"; \
	done; \
	if [ "$$found" -eq 0 ]; then \
		echo "No STL files found in $(INPUT_DIR)"; \
	fi

list-inputs:
	@find "$(INPUT_DIR)" -maxdepth 1 \( -name '*.stl' -o -name '*.STL' \) -print 2>/dev/null || true

clean:
	rm -rf "$(OUTPUT_DIR)"
