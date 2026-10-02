\echo Use "CREATE EXTENSION darmok_server" to load this file. \quit

CREATE FUNCTION begin_catalog_lease(
    OUT lease_id pg_catalog.int8,
    OUT cluster_id pg_catalog.bytea,
    OUT database_oid pg_catalog.oid,
    OUT backend_id pg_catalog.int8,
    OUT generation pg_catalog.int8,
    OUT local_generation pg_catalog.int8
) RETURNS pg_catalog.record
AS 'MODULE_PATHNAME', 'darmok_begin_catalog_lease'
LANGUAGE C VOLATILE PARALLEL UNSAFE;

CREATE FUNCTION check_catalog_lease(lease_id pg_catalog.int8, backend_id pg_catalog.int8)
RETURNS pg_catalog.void
AS 'MODULE_PATHNAME', 'darmok_check_catalog_lease'
LANGUAGE C VOLATILE PARALLEL UNSAFE CALLED ON NULL INPUT;

CREATE FUNCTION end_catalog_lease(lease_id pg_catalog.int8, backend_id pg_catalog.int8)
RETURNS pg_catalog.void
AS 'MODULE_PATHNAME', 'darmok_end_catalog_lease'
LANGUAGE C VOLATILE PARALLEL UNSAFE CALLED ON NULL INPUT;
