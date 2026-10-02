MODULE_big = darmok_catalog_probe
OBJS = darmok_catalog_probe.o
PGFILEDESC = "darmok_catalog_probe - native test controls only"
PG_CFLAGS = -Werror

PG_CONFIG ?= pg_config
PGXS := $(shell $(PG_CONFIG) --pgxs)
include $(PGXS)
