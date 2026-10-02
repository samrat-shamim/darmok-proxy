EXTENSION = darmok_server
MODULE_big = darmok_server
OBJS = darmok_server.o
DATA = darmok_server--1.0.sql
DOCS = LICENSE.darmok_server README.darmok_server.md
PGFILEDESC = "darmok_server - catalog generation and publication leases"
PG_CFLAGS = -Werror

PG_CONFIG ?= pg_config
PGXS := $(shell $(PG_CONFIG) --pgxs)
include $(PGXS)
