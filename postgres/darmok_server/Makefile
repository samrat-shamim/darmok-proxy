EXTENSION = darmok_server
MODULE_big = darmok_server
OBJS = darmok_server.o catalog_read.o heap_storage.o catalog_payload.o module_footprint.o
DATA = darmok_server--1.0.sql
DOCS = LICENSE.darmok_server README.darmok_server.md
HEADERS = statement_guard.h relation_guard.h heap_storage.h module_footprint.h
PGFILEDESC = "darmok_server - catalog discovery and native coordination"
PG_CFLAGS = -Werror

PG_CONFIG ?= pg_config
PGXS := $(shell $(PG_CONFIG) --pgxs)
include $(PGXS)
