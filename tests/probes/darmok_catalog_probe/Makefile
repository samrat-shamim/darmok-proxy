MODULE_big = darmok_catalog_probe
OBJS = darmok_catalog_probe.o
PGFILEDESC = "darmok_catalog_probe - native test controls only"
PG_CFLAGS = -Werror
PG_CPPFLAGS = -I../../../postgres/darmok_server -I$(shell $(PG_CONFIG) --includedir-server)/extension/darmok_server

PG_CONFIG ?= pg_config
PGXS := $(shell $(PG_CONFIG) --pgxs)
include $(PGXS)
