#!/usr/bin/env python3
"""Pinned ordinary stock prepared packets. No proxy handshake implementation."""
import argparse
import hashlib
import importlib.metadata
import json
import os
from pathlib import Path
import platform
import struct
import subprocess

import pymysql


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--container', required=True)
    parser.add_argument('--image', required=True)
    parser.add_argument('--host', default='127.0.0.1')
    parser.add_argument('--port', type=int, required=True)
    parser.add_argument('--corpus', type=Path, required=True)
    parser.add_argument('--evidence-dir', type=Path, required=True)
    args = parser.parse_args()
    args.evidence_dir.mkdir(parents=True, exist_ok=False)
    root = Path(__file__).resolve().parents[2]
    records = []
    connections = []
    corpus = json.loads(args.corpus.read_text())
    result = {'complete': False, 'commands': records, 'cases': [],
              'python': platform.python_version(),
              'pymysql_version': importlib.metadata.version('PyMySQL'),
              'observer_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              'corpus_sha256': hashlib.sha256(args.corpus.read_bytes()).hexdigest()}

    def run(name, argv):
        child = subprocess.run(argv, cwd=root, capture_output=True)
        for part, data in (('stdout', child.stdout), ('stderr', child.stderr)):
            with (args.evidence_dir / (name + '.' + part)).open('xb') as output:
                output.write(data)
        records.append({'name': name, 'argv': argv, 'actual_exit': child.returncode,
                        'stdout_sha256': hashlib.sha256(child.stdout).hexdigest(),
                        'stderr_sha256': hashlib.sha256(child.stderr).hexdigest()})
        if child.returncode != 0:
            raise RuntimeError(name + ' failed')
        return child.stdout.decode().strip()

    def new_connection():
        connection = pymysql.connect(host=args.host, port=args.port, user='root',
                                     password=os.environ['MYSQL_PWD'], charset='utf8mb4', autocommit=True)
        connections.append(connection)
        with connection.cursor() as cursor:
            cursor.execute('SELECT VERSION(), @@server_uuid')
            version, identity = cursor.fetchone()
            if version != corpus['server_version'] or identity != result['server_uuid']:
                raise RuntimeError('connected stock instance differs')
            cursor.execute('SET NAMES utf8mb4 COLLATE utf8mb4_general_ci')
            cursor.execute("SET SESSION sql_mode='' ")
        return connection

    def received(connection):
        return connection._read_packet().get_all_data()

    def command(connection, code, payload=b''):
        connection._execute_command(code, payload)

    def prepare(connection, sql):
        command(connection, 0x16, sql.encode('utf8'))
        head = received(connection)
        if len(head) != 12 or head[0] != 0:
            raise RuntimeError('prepare response differs')
        statement, columns, parameters = struct.unpack_from('<IHH', head, 1)
        packets = [head.hex()]
        for count in (parameters, columns):
            for _ in range(count): packets.append(received(connection).hex())
            if count:
                end = received(connection)
                if len(end) != 5 or end[0] != 0xfe:
                    raise RuntimeError('legacy metadata terminator differs')
                packets.append(end.hex())
        return statement, {'sql': sql, 'columns': columns, 'parameters': parameters, 'packets': packets}

    def execute(connection, statement, bindings=b''):
        command(connection, 0x17, struct.pack('<IBI', statement, 0, 1) + bindings)
        try:
            head = received(connection)
        except pymysql.MySQLError as error:
            return {'error_code': error.args[0], 'message': error.args[1]}
        packets = [head.hex()]
        if len(head) != 1 or not 0 < head[0] < 0xfb:
            raise RuntimeError('selected binary result header differs')
        for _ in range(head[0]): packets.append(received(connection).hex())
        end = received(connection)
        if len(end) != 5 or end[0] != 0xfe:
            raise RuntimeError('selected binary metadata terminator differs')
        packets.append(end.hex())
        while True:
            row = received(connection)
            packets.append(row.hex())
            if len(row) == 5 and row[0] == 0xfe: break
        return {'packets': packets}

    try:
        if result['pymysql_version'] != '1.1.2':
            raise RuntimeError('pinned PyMySQL version required')
        result['source'] = {name: run(name, ['git', 'rev-parse', revision])
                            for name, revision in (('head', 'HEAD'), ('tree', 'HEAD^{tree}'))}
        result['source']['status'] = run('status', ['git', 'status', '--porcelain=v1'])
        if result['source']['status']:
            raise RuntimeError('clean committed observer required')
        result['container'] = json.loads(run('container', ['docker', 'inspect', '--format',
            '{"id":{{json .Id}},"image":{{json .Image}},"started_at":{{json .State.StartedAt}},"restarts":{{json .RestartCount}}}', args.container]))
        result['image'] = json.loads(run('image', ['docker', 'image', 'inspect', '--format',
            '{"id":{{json .Id}},"digests":{{json .RepoDigests}}}', args.image]))
        if args.image != corpus['image_digest'] or args.image not in result['image']['digests'] or result['container']['image'] != result['image']['id']:
            raise RuntimeError('existing fixture differs from pinned image')
        result['server_uuid'] = run('server-uuid', ['docker', 'exec', '-i', '-e', 'MYSQL_PWD', args.container,
            'mysql', '--user=root', '--batch', '--skip-column-names', '--execute', 'SELECT @@server_uuid'])
        for index, case in enumerate(corpus['cases']):
            connection = new_connection()
            statement, prepared = prepare(connection, case['prepare']['sql'])
            actual = {'prepare': prepared, 'executions': []}
            result['cases'].append(actual)
            if prepared != case['prepare']:
                raise RuntimeError('prepare packets differ in case ' + str(index))
            for execution_index, expected in enumerate(case['executions']):
                if execution_index == 1 and index not in (1, 4):
                    with connection.cursor() as cursor:
                        cursor.execute("SET SESSION sql_mode='ANSI_QUOTES,NO_BACKSLASH_ESCAPES'")
                        cursor.execute('SET autocommit=0')
                if index == 1 and execution_index == 2:
                    command(connection, 0x1a, struct.pack('<I', statement))
                    actual['reset'] = received(connection).hex()
                    if actual['reset'] != case['reset']:
                        raise RuntimeError('reset response differs')
                observed = execute(connection, statement, bytes.fromhex(expected.get('bindings', '')))
                actual['executions'].append({'response': observed})
                if observed != expected['response']:
                    raise RuntimeError('execution packets differ in case ' + str(index))
            command(connection, 0x19, struct.pack('<I', statement))
            if 'after_close' in case:
                actual['after_close'] = execute(connection, statement)
                if actual['after_close'] != case['after_close']:
                    raise RuntimeError('close outcome differs')
        for expected in corpus['condition_counts']:
            connection = new_connection()
            statement, _ = prepare(connection, 'SELECT 1')
            if execute(connection, 9999).get('error_code') != 1243:
                raise RuntimeError('unknown statement outcome differs')
            action = expected['action']
            if action == 'prepare': prepare(connection, 'SELECT 2')
            elif action in ('close', 'unknown-close'):
                command(connection, 0x19, struct.pack('<I', statement if action == 'close' else 9999))
            elif action == 'reset':
                command(connection, 0x1a, struct.pack('<I', statement))
                received(connection)
            else: raise RuntimeError('unknown condition-count case')
            command(connection, 0x0e)
            if received(connection).hex() != expected['ping']:
                raise RuntimeError('condition count differs')
        result['condition_cases'] = len(corpus['condition_counts'])
        result['complete'] = True
    finally:
        for connection in connections: connection.close()
        with (args.evidence_dir / 'observations.json').open('x') as output:
            json.dump(result, output, indent=2, ensure_ascii=False)
            output.write('\n')
    print(json.dumps({'complete': result['complete'], 'cases': len(result['cases']),
                      'condition_cases': result['condition_cases']}))


if __name__ == '__main__':
    main()
