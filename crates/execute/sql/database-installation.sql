DO $darmok_installation$
DECLARE
    allow_create constant pg_catalog.bool := true;
    schema_oid pg_catalog.oid;
    server_schema_oid pg_catalog.oid;
    extension_oid pg_catalog.oid;
    table_oid pg_catalog.oid;
    index_oid pg_catalog.oid;
    row_type_oid pg_catalog.oid;
    array_type_oid pg_catalog.oid;
    function_oids pg_catalog.oid[] := ARRAY[]::pg_catalog.oid[];
    function_oid pg_catalog.oid;
    i pg_catalog.int4;
    names constant pg_catalog.text[] := ARRAY[
        'substring_utf8', 'substring_utf8', 'substring_bytes', 'substring_bytes'
    ];
    arguments constant pg_catalog.text[] := ARRAY[
        'pg_catalog.text, pg_catalog.int8, pg_catalog.int8',
        'pg_catalog.text, pg_catalog.int8',
        'pg_catalog.bytea, pg_catalog.int8, pg_catalog.int8',
        'pg_catalog.bytea, pg_catalog.int8'
    ];
    argument_oids constant pg_catalog.text[] := ARRAY['25 20 20', '25 20', '17 20 20', '17 20'];
    returns constant pg_catalog.text[] := ARRAY[
        'pg_catalog.text', 'pg_catalog.text', 'pg_catalog.bytea', 'pg_catalog.bytea'
    ];
    return_oids constant pg_catalog.oid[] := ARRAY[25, 25, 17, 17]::pg_catalog.oid[];
    bodies constant pg_catalog.text[] := ARRAY[
$utf8_three$SELECT CASE
    WHEN $2 OPERATOR(pg_catalog.=) 0 OR $3 OPERATOR(pg_catalog.<=) 0
        OR $2 OPERATOR(pg_catalog.<) OPERATOR(pg_catalog.-) n THEN ''::pg_catalog.text
    ELSE pg_catalog.substr($1,
        (CASE WHEN $2 OPERATOR(pg_catalog.>) 0 THEN
            CASE WHEN $2 OPERATOR(pg_catalog.>) n THEN n OPERATOR(pg_catalog.+) 1 ELSE $2 END
        ELSE n OPERATOR(pg_catalog.+) $2 OPERATOR(pg_catalog.+) 1 END)::pg_catalog.int4,
        (CASE WHEN $3 OPERATOR(pg_catalog.>) n THEN n ELSE $3 END)::pg_catalog.int4)
    END
FROM (SELECT pg_catalog.char_length($1)::pg_catalog.int8 AS n) AS size$utf8_three$,
$utf8_two$SELECT darmok.substring_utf8($1, $2, 9223372036854775807::pg_catalog.int8)$utf8_two$,
$bytes_three$SELECT CASE
    WHEN $2 OPERATOR(pg_catalog.=) 0 OR $3 OPERATOR(pg_catalog.<=) 0
        OR $2 OPERATOR(pg_catalog.<) OPERATOR(pg_catalog.-) n THEN ''::pg_catalog.bytea
    ELSE pg_catalog.substr($1,
        (CASE WHEN $2 OPERATOR(pg_catalog.>) 0 THEN
            CASE WHEN $2 OPERATOR(pg_catalog.>) n THEN n OPERATOR(pg_catalog.+) 1 ELSE $2 END
        ELSE n OPERATOR(pg_catalog.+) $2 OPERATOR(pg_catalog.+) 1 END)::pg_catalog.int4,
        (CASE WHEN $3 OPERATOR(pg_catalog.>) n THEN n ELSE $3 END)::pg_catalog.int4)
    END
FROM (SELECT pg_catalog.octet_length($1)::pg_catalog.int8 AS n) AS size$bytes_three$,
$bytes_two$SELECT darmok.substring_bytes($1, $2, 9223372036854775807::pg_catalog.int8)$bytes_two$
    ];
BEGIN
    -- This lock serializes these explicit operations in this database. It is
    -- not a lease against arbitrary application DDL or a runtime catalog guard.
    IF allow_create THEN
        PERFORM pg_catalog.pg_advisory_xact_lock(4922526098346491905::pg_catalog.int8);
    ELSE
        PERFORM pg_catalog.pg_advisory_xact_lock_shared(4922526098346491905::pg_catalog.int8);
    END IF;
    IF pg_catalog.current_setting('server_version_num')::pg_catalog.int4 / 10000
        NOT IN (17, 18) OR pg_catalog.current_setting('server_encoding') <> 'UTF8' THEN
        RAISE EXCEPTION 'Darmok schema requires PostgreSQL 17 or 18 with UTF8 encoding';
    END IF;
    SELECT oid INTO schema_oid FROM pg_catalog.pg_namespace
        WHERE nspname::pg_catalog.text COLLATE pg_catalog."C" = 'darmok';
    IF schema_oid IS NULL THEN
        IF NOT allow_create THEN
            RAISE EXCEPTION 'Darmok schema is missing; explicit initialization is required';
        END IF;
        CREATE SCHEMA darmok;
        CREATE TABLE darmok.installation (
            singleton pg_catalog.bool PRIMARY KEY CHECK (singleton),
            format_version pg_catalog.int4 NOT NULL,
            profile pg_catalog.text COLLATE pg_catalog."C" NOT NULL
        );
        INSERT INTO darmok.installation VALUES
            (true, 1, 'substring-signed64-utf8-bytes-v1');
        FOR i IN 1..4 LOOP
            EXECUTE pg_catalog.format(
                'CREATE FUNCTION darmok.%I(%s) RETURNS %s LANGUAGE sql IMMUTABLE STRICT PARALLEL SAFE AS %L',
                names[i], arguments[i], returns[i], bodies[i]
            );
        END LOOP;
        SELECT oid INTO STRICT schema_oid FROM pg_catalog.pg_namespace
            WHERE nspname::pg_catalog.text COLLATE pg_catalog."C" = 'darmok';
    END IF;

    -- Validate the table before reading its data. A view, foreign table or
    -- partial installation is a conflict, never a source for implicit repair.
    SELECT oid, reltype INTO table_oid, row_type_oid FROM pg_catalog.pg_class
        WHERE relnamespace = schema_oid AND relname::pg_catalog.text COLLATE pg_catalog."C" = 'installation'
            AND relkind = 'r' AND relpersistence = 'p' AND NOT relispartition AND reloftype = 0;
    IF table_oid IS NULL OR (SELECT pg_catalog.count(*) FROM pg_catalog.pg_attribute
            WHERE attrelid = table_oid AND attnum > 0) <> 3
        OR EXISTS (SELECT FROM pg_catalog.pg_attribute WHERE attrelid = table_oid AND attnum > 0
            AND (attisdropped OR NOT attnotnull OR atttypmod <> -1 OR attndims <> 0
                OR atthasdef OR attidentity <> '' OR attgenerated <> ''
                OR NOT ((attnum = 1 AND attname = 'singleton' AND atttypid = 16 AND attcollation = 0)
                    OR (attnum = 2 AND attname = 'format_version' AND atttypid = 23 AND attcollation = 0)
                    OR (attnum = 3 AND attname = 'profile' AND atttypid = 25
                        AND attcollation = 'pg_catalog."C"'::pg_catalog.regcollation))))
        OR EXISTS (SELECT FROM pg_catalog.pg_inherits WHERE inhrelid = table_oid OR inhparent = table_oid)
        OR EXISTS (SELECT FROM pg_catalog.pg_trigger WHERE tgrelid = table_oid)
        OR EXISTS (SELECT FROM pg_catalog.pg_rewrite WHERE ev_class = table_oid)
        OR EXISTS (SELECT FROM pg_catalog.pg_attrdef WHERE adrelid = table_oid) THEN
        RAISE EXCEPTION 'Darmok installation table definition does not match';
    END IF;
    IF (SELECT pg_catalog.count(*) FROM pg_catalog.pg_constraint
            WHERE conrelid = table_oid AND contype <> 'n') <> 2
        OR NOT EXISTS (SELECT FROM pg_catalog.pg_constraint WHERE conrelid = table_oid
            AND contype = 'p' AND conkey = ARRAY[1]::pg_catalog.int2[]
            AND NOT condeferrable AND NOT condeferred AND convalidated)
        OR NOT EXISTS (SELECT FROM pg_catalog.pg_constraint WHERE conrelid = table_oid
            AND contype = 'c' AND conkey = ARRAY[1]::pg_catalog.int2[]
            AND NOT condeferrable AND NOT condeferred AND convalidated AND NOT connoinherit
            AND pg_catalog.pg_get_constraintdef(oid, false) COLLATE pg_catalog."C" = 'CHECK (singleton)') THEN
        RAISE EXCEPTION 'Darmok installation constraints do not match';
    END IF;
    -- PostgreSQL 18 represents NOT NULL constraints separately; attnotnull
    -- alone is not proof that a newly added constraint has been validated.
    IF (SELECT pg_catalog.count(*) FROM pg_catalog.pg_constraint
            WHERE conrelid = table_oid AND contype = 'n') <>
            (CASE WHEN pg_catalog.current_setting('server_version_num')::pg_catalog.int4 / 10000 = 18 THEN 3 ELSE 0 END)
        OR EXISTS (SELECT FROM pg_catalog.pg_constraint AS c WHERE c.conrelid = table_oid
            AND (NOT c.convalidated OR NOT c.conislocal OR c.coninhcount <> 0
                OR NOT COALESCE((pg_catalog.to_jsonb(c)->>'conenforced')::pg_catalog.bool, true)
                OR (c.contype = 'n' AND c.conkey NOT IN
                    (ARRAY[1]::pg_catalog.int2[], ARRAY[2]::pg_catalog.int2[], ARRAY[3]::pg_catalog.int2[])))) THEN
        RAISE EXCEPTION 'Darmok installation constraints do not match';
    END IF;
    SELECT c.oid INTO index_oid FROM pg_catalog.pg_class AS c
        JOIN pg_catalog.pg_index AS x ON x.indexrelid = c.oid
        JOIN pg_catalog.pg_am AS a ON a.oid = c.relam
        WHERE c.relnamespace = schema_oid AND c.relname = 'installation_pkey'
            AND c.relkind = 'i' AND c.relpersistence = 'p' AND NOT c.relispartition
            AND x.indrelid = table_oid AND x.indisunique AND x.indisprimary
            AND x.indisvalid AND x.indisready AND x.indislive AND x.indimmediate
            AND NOT x.indnullsnotdistinct
            AND x.indnatts = 1 AND x.indnkeyatts = 1 AND x.indkey::pg_catalog.text = '1'
            AND x.indpred IS NULL AND x.indexprs IS NULL AND x.indoption::pg_catalog.text = '0'
            AND x.indcollation::pg_catalog.text = '0' AND a.amname = 'btree'
            AND x.indclass[0] = (SELECT oid FROM pg_catalog.pg_opclass
                WHERE opcnamespace = 'pg_catalog'::pg_catalog.regnamespace AND opcname = 'bool_ops'
                    AND opcmethod = a.oid);
    SELECT typarray INTO array_type_oid FROM pg_catalog.pg_type
        WHERE oid = row_type_oid AND typnamespace = schema_oid AND typname = 'installation'
            AND typtype = 'c' AND typrelid = table_oid;
    IF index_oid IS NULL OR array_type_oid IS NULL
        OR NOT EXISTS (SELECT FROM pg_catalog.pg_type WHERE oid = array_type_oid
            AND typnamespace = schema_oid AND typname = '_installation' AND typelem = row_type_oid)
        OR (SELECT pg_catalog.count(*) FROM pg_catalog.pg_class WHERE relnamespace = schema_oid) <> 2
        OR (SELECT pg_catalog.count(*) FROM pg_catalog.pg_type WHERE typnamespace = schema_oid) <> 2 THEN
        RAISE EXCEPTION 'Darmok relation or type inventory does not match';
    END IF;
    FOR i IN 1..4 LOOP
        SELECT p.oid INTO function_oid FROM pg_catalog.pg_proc AS p
            JOIN pg_catalog.pg_language AS l ON l.oid = p.prolang
            WHERE p.pronamespace = schema_oid AND p.proname::pg_catalog.text COLLATE pg_catalog."C" = names[i]
                AND p.proargtypes::pg_catalog.text COLLATE pg_catalog."C" = argument_oids[i]
                AND p.prorettype = return_oids[i] AND l.lanname = 'sql'
                AND p.prokind = 'f' AND p.provolatile = 'i' AND p.proisstrict AND p.proparallel = 's'
                AND NOT p.proretset AND p.provariadic = 0 AND p.pronargdefaults = 0
                AND p.proargnames IS NULL AND p.proallargtypes IS NULL AND p.proargmodes IS NULL
                AND p.proargdefaults IS NULL AND p.proconfig IS NULL AND p.prosqlbody IS NULL AND p.prosupport = 0
                AND p.prosrc COLLATE pg_catalog."C" = bodies[i];
        IF function_oid IS NULL THEN
            RAISE EXCEPTION 'Darmok compatibility function definition does not match';
        END IF;
        function_oids := pg_catalog.array_append(function_oids, function_oid);
    END LOOP;
    IF (SELECT pg_catalog.count(*) FROM pg_catalog.pg_proc WHERE pronamespace = schema_oid) <> 4
        OR EXISTS (SELECT FROM pg_catalog.pg_depend AS d
            WHERE d.refclassid = 'pg_catalog.pg_namespace'::pg_catalog.regclass
                AND d.refobjid = schema_oid
                AND NOT ((d.classid = 'pg_catalog.pg_class'::pg_catalog.regclass
                        AND d.objid IN (table_oid, index_oid))
                    OR (d.classid = 'pg_catalog.pg_type'::pg_catalog.regclass
                        AND d.objid IN (row_type_oid, array_type_oid))
                    OR (d.classid = 'pg_catalog.pg_proc'::pg_catalog.regclass
                        AND d.objid = ANY(function_oids)))) THEN
        RAISE EXCEPTION 'Darmok schema contains unexpected objects';
    END IF;
    IF (SELECT pg_catalog.count(*) FROM darmok.installation) <> 1
        OR NOT EXISTS (SELECT FROM darmok.installation
            WHERE singleton AND format_version = 1
                AND profile COLLATE pg_catalog."C" = 'substring-signed64-utf8-bytes-v1') THEN
        RAISE EXCEPTION 'Darmok installation version or metadata does not match';
    END IF;

    SELECT oid INTO server_schema_oid FROM pg_catalog.pg_namespace
        WHERE nspname::pg_catalog.text COLLATE pg_catalog."C" = 'darmok_server';
    SELECT oid INTO extension_oid FROM pg_catalog.pg_extension
        WHERE extname::pg_catalog.text COLLATE pg_catalog."C" = 'darmok_server';
    IF extension_oid IS NULL THEN
        IF NOT allow_create THEN
            RAISE EXCEPTION 'Darmok server extension is missing; explicit initialization is required';
        END IF;
        IF server_schema_oid IS NOT NULL THEN
            RAISE EXCEPTION 'Darmok server namespace exists without its extension';
        END IF;
        CREATE EXTENSION darmok_server VERSION '1.0';
        SELECT oid INTO STRICT extension_oid FROM pg_catalog.pg_extension
            WHERE extname::pg_catalog.text COLLATE pg_catalog."C" = 'darmok_server';
        SELECT oid INTO STRICT server_schema_oid FROM pg_catalog.pg_namespace
            WHERE nspname::pg_catalog.text COLLATE pg_catalog."C" = 'darmok_server';
    END IF;
    IF NOT EXISTS (SELECT FROM pg_catalog.pg_extension
            WHERE oid = extension_oid AND extnamespace = server_schema_oid
                AND extversion COLLATE pg_catalog."C" = '1.0' AND NOT extrelocatable
                AND extconfig IS NULL AND extcondition IS NULL)
        OR (SELECT pg_catalog.count(*) FROM pg_catalog.pg_depend
            WHERE classid = 'pg_catalog.pg_extension'::pg_catalog.regclass
                AND objid = extension_oid) <> 1
        OR NOT EXISTS (SELECT FROM pg_catalog.pg_depend
            WHERE classid = 'pg_catalog.pg_extension'::pg_catalog.regclass
                AND objid = extension_oid AND objsubid = 0
                AND refclassid = 'pg_catalog.pg_namespace'::pg_catalog.regclass
                AND refobjid = server_schema_oid AND refobjsubid = 0 AND deptype = 'n') THEN
        RAISE EXCEPTION 'Darmok server extension definition does not match';
    END IF;
    -- The shipped extension creates no SQL objects. Ordinary incoming
    -- application dependencies are not extension membership and are allowed.
    IF EXISTS (SELECT FROM pg_catalog.pg_depend
            WHERE refclassid = 'pg_catalog.pg_extension'::pg_catalog.regclass
                AND refobjid = extension_oid AND deptype = 'e') THEN
        RAISE EXCEPTION 'Darmok server extension contains unexpected members';
    END IF;
    IF EXISTS (SELECT FROM pg_catalog.pg_depend
            WHERE refclassid = 'pg_catalog.pg_namespace'::pg_catalog.regclass
                AND refobjid = server_schema_oid
                AND NOT (classid = 'pg_catalog.pg_extension'::pg_catalog.regclass
                    AND objid = extension_oid AND objsubid = 0
                    AND refobjsubid = 0 AND deptype = 'n')) THEN
        RAISE EXCEPTION 'Darmok server namespace contains unexpected objects';
    END IF;
END
$darmok_installation$;
