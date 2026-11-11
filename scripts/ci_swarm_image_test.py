"""Offline contract tests for the production swarm-image boundary controller."""

import copy
import gzip
import io
import json
import re
import sys
from pathlib import Path
import tarfile
import tempfile
import unittest
import urllib.error
import warnings
from unittest.mock import patch
import zipfile

sys.path.insert(0, str(Path(__file__).resolve().parent))
import ci_swarm_image as image

SOURCE = 'a' * 40
TREE = 'b' * 40
DIGEST = 'sha256:' + 'c' * 64
OTHER = 'sha256:' + 'd' * 64


def context(event='workflow_dispatch'):
    return dict(repository=image.REPOSITORY, ref='refs/heads/main', sha=SOURCE,
                workflow_sha=SOURCE, event_name=event, run_id='11', run_attempt='1')


def request(move=False):
    return dict(source_sha=SOURCE, source_tree=TREE, qualification_run_id='10',
                artifact_id='20', archive_sha256=DIGEST, manifest_digest=DIGEST,
                move_main='true' if move else 'false',
                expected_main_digest=OTHER if move else '')


def provenance():
    run = dict(repository={'full_name': image.REPOSITORY}, id=10,
               path=image.QUALIFICATION_WORKFLOW, event='push', head_branch='main',
               status='completed', conclusion='success', head_sha=SOURCE, run_attempt=1)
    artifact = dict(id=20, name=f'swarm-image-{SOURCE}-10-1', expired=False,
                    size_in_bytes=100, digest=DIGEST,
                    workflow_run=dict(id=10, head_sha=SOURCE))
    return run, artifact


def oci_members(platform='amd64', layout='1.0.0', extra=False, descriptor_size=False,
                manifests=1, layer_bytes=b'synthetic layer', compressed=False,
                bad_diff=False, revision=SOURCE):
    members = {}

    def blob(value):
        data = json.dumps(value).encode() if isinstance(value, dict) else value
        digest = image.sha256(data)
        members['blobs/sha256/' + digest.split(':')[1]] = data
        return dict(digest=digest, size=len(data))

    layer = blob(gzip.compress(layer_bytes, mtime=0) if compressed else layer_bytes)
    layer['mediaType'] = 'application/vnd.oci.image.layer.v1.tar' + ('+gzip' if compressed else '')
    config = blob(dict(os='linux', architecture=platform,
                       rootfs={'type': 'layers', 'diff_ids': [OTHER if bad_diff else image.sha256(layer_bytes)]},
                       config={'Labels': {
        'org.opencontainers.image.revision': revision,
        'org.opencontainers.image.source': f'https://github.com/{image.REPOSITORY}'}}))
    manifest = blob(dict(schemaVersion=2, mediaType='application/vnd.oci.image.manifest.v1+json',
                         config=config, layers=[layer]))
    if descriptor_size:
        manifest['size'] += 1
    members['oci-layout'] = json.dumps({'imageLayoutVersion': layout}).encode()
    members['index.json'] = json.dumps(dict(schemaVersion=2, manifests=[manifest] * manifests)).encode()
    if extra:
        blob(b'unreferenced')
    return members


def archive(path, members, duplicate=None, link=None):
    with tarfile.open(path, 'w') as output:
        for name, data in list(members.items()) + ([(duplicate, members[duplicate])] if duplicate else []):
            item = tarfile.TarInfo(name)
            item.size = len(data)
            output.addfile(item, io.BytesIO(data))
        if link:
            item = tarfile.TarInfo(link)
            item.type = tarfile.SYMTYPE
            item.linkname = '/tmp/untrusted'
            output.addfile(item)


class OCIContract(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.path = Path(self.temp.name) / 'image.tar'

    def test_valid_exact_archive(self):
        archive(self.path, oci_members())
        identity = image.inspect_oci(self.path)
        self.assertEqual(identity['platform'], 'linux/amd64')
        self.assertEqual(identity['labels']['org.opencontainers.image.revision'], SOURCE)
        self.assertRegex(identity['manifest_digest'], image.DIGEST)

    def test_structural_negatives(self):
        cases = [dict(platform='arm64'), dict(layout='2.0.0'), dict(extra=True),
                 dict(descriptor_size=True), dict(manifests=0), dict(manifests=2)]
        for case in cases:
            with self.subTest(case=case):
                archive(self.path, oci_members(**case))
                with self.assertRaises(ValueError):
                    image.inspect_oci(self.path)

    def test_layer_diff_ids_and_expansion_bounds(self):
        for compressed in (False, True):
            with self.subTest(compressed=compressed):
                archive(self.path, oci_members(compressed=compressed))
                self.assertEqual(image.inspect_oci(self.path)['layer_diff_ids'],
                                 [image.sha256(b'synthetic layer')])
                archive(self.path, oci_members(compressed=compressed, bad_diff=True))
                with self.assertRaisesRegex(ValueError, 'diff IDs'):
                    image.inspect_oci(self.path)
        archive(self.path, oci_members(compressed=True, layer_bytes=b'x' * 100000))
        with patch.object(image, 'MAX_ARCHIVE', self.path.stat().st_size):
            with self.assertRaisesRegex(ValueError, 'expanded image layers'):
                image.inspect_oci(self.path)

    def test_corrupt_blob(self):
        members = oci_members()
        name = next(name for name in members if name.startswith('blobs/'))
        members[name] += b'changed'
        archive(self.path, members)
        with self.assertRaisesRegex(ValueError, 'corrupt'):
            image.inspect_oci(self.path)

    def test_missing_blob(self):
        members = oci_members()
        del members[next(iter(members))]
        archive(self.path, members)
        with self.assertRaises(ValueError):
            image.inspect_oci(self.path)

    def test_links_duplicates_traversal_and_extra(self):
        for kind in ('link', 'duplicate', 'traversal', 'extra'):
            with self.subTest(kind=kind):
                members = oci_members()
                if kind in ('traversal', 'extra'):
                    members['../escape' if kind == 'traversal' else 'script.sh'] = b'bad'
                archive(self.path, members, duplicate='index.json' if kind == 'duplicate' else None,
                        link='link' if kind == 'link' else None)
                with self.assertRaises(ValueError):
                    image.inspect_oci(self.path)


class IdentityContract(unittest.TestCase):
    def test_context_rejections(self):
        image.validate_context(context(), 'publish')
        for key, value in dict(repository='other/repo', ref='refs/heads/feature', sha='main',
                               workflow_sha='b' * 40, event_name='push', run_id='0', run_attempt='').items():
            with self.subTest(key=key):
                ctx = context()
                ctx[key] = value
                with self.assertRaises(ValueError):
                    image.validate_context(ctx, 'publish')

    def test_request_rejections(self):
        image.validate_request(request(), context())
        image.validate_request(request(True), context())
        for key, value in dict(source_sha='b' * 40, source_tree='main', qualification_run_id='0',
                               artifact_id='../20', archive_sha256='bad', manifest_digest='bad',
                               move_main='', expected_main_digest=OTHER).items():
            with self.subTest(key=key):
                candidate = request()
                candidate[key] = value
                with self.assertRaises(ValueError):
                    image.validate_request(candidate, context())
        candidate = request(True)
        candidate['expected_main_digest'] = ''
        with self.assertRaises(ValueError):
            image.validate_request(candidate, context())

    def test_omitted_optional_input_is_canonical_only_for_immutable_policy(self):
        req = request()
        del req['expected_main_digest']
        normalized = image.validate_request(req, context())
        self.assertEqual(normalized['expected_main_digest'], '')
        self.assertNotIn('expected_main_digest', req)
        req['move_main'] = 'true'
        with self.assertRaisesRegex(ValueError, 'main movement'):
            image.validate_request(req, context())
        req['move_main'] = 'false'
        req['unknown'] = 'refuse extra input'
        with self.assertRaisesRegex(ValueError, 'fields'):
            image.validate_request(req, context())

    def test_run_and_artifact_rejections(self):
        run, artifact = provenance()
        image.validate_provenance(request(), run, artifact)
        for key, value in dict(repository={'full_name': 'other/repo'}, id=99, path='other.yml',
                               event='pull_request', head_branch='feature', status='in_progress',
                               conclusion='failure', head_sha='b' * 40, run_attempt=2).items():
            with self.subTest(run_key=key):
                candidate = copy.deepcopy(run)
                candidate[key] = value
                with self.assertRaises(ValueError):
                    image.validate_provenance(request(), candidate, artifact)
        for key, value in dict(id=99, name='wrong', expired=True, size_in_bytes=0,
                               digest='bad', workflow_run={'id': 99, 'head_sha': SOURCE}).items():
            with self.subTest(artifact_key=key):
                candidate = copy.deepcopy(artifact)
                candidate[key] = value
                with self.assertRaises(ValueError):
                    image.validate_provenance(request(), run, candidate)

    def test_receipt_identity_and_smoke(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'image.tar'
            archive(path, oci_members())
            identity = image.inspect_oci(path)
            req = request()
            req.update(manifest_digest=identity['manifest_digest'], archive_sha256=image.file_digest(path))
            run, _ = provenance()
            receipt = dict(schema='tokmd.swarm_image_qualification.v1', result='qualified', registry_write=False,
                           repository=image.REPOSITORY, source_sha=SOURCE, source_tree=TREE,
                           workflow=image.QUALIFICATION_WORKFLOW, run_id='10', run_attempt='1',
                           artifact_name=f'swarm-image-{SOURCE}-10-1',
                           event='push', ref='refs/heads/main', manifest_digest=req['manifest_digest'],
                           archive_sha256=req['archive_sha256'], config_digest=identity['config_digest'],
                           platform='linux/amd64', archive_bytes=path.stat().st_size,
                           layer_diff_ids=identity['layer_diff_ids'], daemon_image_id=OTHER,
                           loaded_manifest_digest=DIGEST, loaded_identity_verified=True,
                           smoke=[dict(argument=arg, exit_code=0, stdout_sha256=DIGEST)
                                  for arg in ('--version', '--help')])
            image.validate_receipt(req, run, receipt, identity, path)
            run['event'] = 'workflow_dispatch'
            with self.assertRaisesRegex(ValueError, 'identity mismatch'):
                image.validate_receipt(req, run, receipt, identity, path)
            receipt['event'] = 'workflow_dispatch'
            image.validate_receipt(req, run, receipt, identity, path)
            for key in receipt:
                with self.subTest(key=key):
                    candidate = copy.deepcopy(receipt)
                    candidate[key] = [] if key == 'smoke' else 'wrong'
                    with self.assertRaises(ValueError):
                        image.validate_receipt(req, run, candidate, identity, path)

    def test_manual_main_qualification_preserves_provenance(self):
        run, artifact = provenance()
        run['event'] = 'workflow_dispatch'
        image.validate_provenance(request(), run, artifact)
        run['head_branch'] = 'feature'
        with self.assertRaisesRegex(ValueError, 'protected main'):
            image.validate_provenance(request(), run, artifact)

    def test_zip_identity_and_members(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for number, names in enumerate([['image.tar', 'qualification.json'],
                                            ['../image.tar', 'qualification.json'],
                                            ['image.tar', 'image.tar'], ['image.tar']]):
                path = root / f'{number}.zip'
                with warnings.catch_warnings():
                    warnings.simplefilter('ignore', UserWarning)
                    with zipfile.ZipFile(path, 'w') as output:
                        for name in names:
                            output.writestr(name, b'bytes')
                destination = root / str(number)
                destination.mkdir()
                if number == 0:
                    with self.assertRaisesRegex(ValueError, 'ZIP digest'):
                        image.unpack_artifact(path, destination, OTHER)
                    image.unpack_artifact(path, destination, image.file_digest(path))
                    self.assertEqual((destination / 'image.tar').read_bytes(), b'bytes')
                else:
                    with self.assertRaises(ValueError):
                        image.unpack_artifact(path, destination, image.file_digest(path))


class RegistryContract(unittest.TestCase):
    def test_only_manifest_unknown_404_is_absence(self):
        for status, code in ((404, 'MANIFEST_UNKNOWN'), (404, 'NAME_UNKNOWN'),
                             (401, 'UNAUTHORIZED'), (403, 'DENIED'), (500, 'UNKNOWN')):
            with self.subTest(status=status, code=code):
                error = urllib.error.HTTPError('https://ghcr.io', status, 'error', {},
                    io.BytesIO(json.dumps({'errors': [{'code': code, 'message': 'manifest unknown'}]}).encode()))
                with patch.object(image.urllib.request, 'urlopen', side_effect=error):
                    if status == 404 and code == 'MANIFEST_UNKNOWN':
                        self.assertIsNone(image.registry_manifest('main', 'test-token'))
                    else:
                        with self.assertRaises(ValueError):
                            image.registry_manifest('main', 'test-token')

    def test_negotiation_failure_is_not_absence(self):
        for message in ('OCI index found, but Accept header does not support OCI indexes',
                        'unknown future response', None):
            payload = {'errors': [{'code': 'MANIFEST_UNKNOWN', 'message': message}]}
            error = urllib.error.HTTPError('https://ghcr.io', 404, 'error', {},
                                          io.BytesIO(json.dumps(payload).encode()))
            with self.subTest(message=message), \
                 patch.object(image.urllib.request, 'urlopen', side_effect=error):
                with self.assertRaisesRegex(ValueError, 'absence is unproved'):
                    image.registry_manifest('main', 'test-token')

    def test_existing_index_tags_cannot_be_overwritten_as_absent(self):
        index = json.dumps({'schemaVersion': 2, 'mediaType':
                            'application/vnd.oci.image.index.v1+json', 'manifests': []}).encode()
        for existing in ('main', 'sha-' + SOURCE):
            req = request(existing == 'main')
            if existing == 'main':
                req['expected_main_digest'] = 'absent'

            def fetch(native, timeout):
                accepted = native.get_header('Accept')
                self.assertIn('application/vnd.oci.image.index.v1+json', accepted)
                self.assertIn('application/vnd.docker.distribution.manifest.list.v2+json', accepted)
                if native.full_url.rsplit('/', 1)[1] != existing:
                    payload = {'errors': [{'code': 'MANIFEST_UNKNOWN', 'message': 'manifest unknown'}]}
                    raise urllib.error.HTTPError(native.full_url, 404, 'Not Found', {},
                                                 io.BytesIO(json.dumps(payload).encode()))
                response = io.BytesIO(index)
                response.headers = {'Docker-Content-Digest': image.sha256(index)}
                return response

            with self.subTest(existing=existing), tempfile.TemporaryDirectory() as directory, \
                 patch.object(image.urllib.request, 'urlopen', side_effect=fetch), \
                 patch('builtins.print'), patch.object(image, 'command') as copy:
                checkpoint = Path(directory) / 'publication.json'
                with self.assertRaises(ValueError):
                    image.publish_verified(req, 'qualified.tar', checkpoint,
                                           lambda tag: image.registry_manifest(tag, 'test-token'),
                                           copy, lambda: None)
                copy.assert_not_called()
                self.assertEqual(json.loads(checkpoint.read_text())['state'], 'validating_registry')

    def test_registry_digest_matches_bytes_and_header(self):
        body = b'{"schemaVersion":2}'
        for header in (image.sha256(body), OTHER):
            with self.subTest(header=header):
                response = io.BytesIO(body)
                response.headers = {'Docker-Content-Digest': header}
                with patch.object(image.urllib.request, 'urlopen', return_value=response):
                    if header == OTHER:
                        with self.assertRaises(ValueError):
                            image.registry_manifest('main', 'test-token')
                    else:
                        self.assertEqual(image.registry_manifest('main', 'test-token'), header)


class PublisherContract(unittest.TestCase):
    def exercise(self, move=False, initial=None, lookup_hook=None, copy_hook=None, source_hook=None):
        req = request(move)
        registry = dict(initial or {})
        if move and 'main' not in registry:
            registry['main'] = OTHER
        writes, reads, sources = [], [], []
        with tempfile.TemporaryDirectory() as directory:
            receipt_path = Path(directory) / 'publication.json'

            def lookup(tag):
                reads.append(tag)
                if lookup_hook:
                    lookup_hook(tag, registry, reads)
                return registry.get(tag)

            def publish(tag, path):
                writes.append((tag, path))
                if copy_hook:
                    copy_hook(tag, registry)
                else:
                    registry[tag] = DIGEST

            def source():
                sources.append(True)
                if source_hook:
                    source_hook(len(sources), registry)

            error = None
            try:
                with patch('builtins.print'):
                    image.publish_verified(req, 'qualified.tar', receipt_path, lookup, publish, source)
            except BaseException as caught:
                error = caught
            receipt = json.loads(receipt_path.read_text())
        return error, receipt, writes, reads

    def test_success_immutable_and_main(self):
        for move in (False, True):
            with self.subTest(move=move):
                error, receipt, writes, _ = self.exercise(move)
                self.assertIsNone(error)
                self.assertEqual(receipt['state'], 'complete')
                self.assertEqual(writes, [('sha-' + SOURCE, 'qualified.tar')] +
                                 ([('main', 'qualified.tar')] if move else []))

    def test_duplicate_and_conflicting_immutable(self):
        for digest in (DIGEST, OTHER):
            with self.subTest(digest=digest):
                error, receipt, writes, _ = self.exercise(initial={'sha-' + SOURCE: digest})
                self.assertIsInstance(error, ValueError)
                self.assertEqual(writes, [])
                self.assertEqual(receipt['terminal'], 'failed_or_interrupted')

    def test_lookup_failure_is_not_absence(self):
        def unavailable(*args):
            raise TimeoutError('offline')
        error, _, writes, _ = self.exercise(lookup_hook=unavailable)
        self.assertIsInstance(error, TimeoutError)
        self.assertEqual(writes, [])

    def test_tag_appears_before_first_write(self):
        def race(tag, registry, reads):
            if len(reads) == 2:
                registry[tag] = OTHER
        error, _, writes, _ = self.exercise(lookup_hook=race)
        self.assertIsInstance(error, ValueError)
        self.assertEqual(writes, [])

    def test_source_moves_before_either_write(self):
        for when in (1, 2, 3):
            with self.subTest(when=when):
                def moved(count, registry):
                    if count == when:
                        raise ValueError('main moved')
                error, receipt, writes, _ = self.exercise(True, source_hook=moved)
                self.assertIsInstance(error, ValueError)
                self.assertEqual(len(writes), 1 if when == 3 else 0)
                self.assertEqual(receipt['state'], 'immutable_verified' if when == 3 else 'validating_registry')

    def test_stale_main_before_and_after_immutable(self):
        error, _, writes, _ = self.exercise(True, initial={'main': DIGEST})
        self.assertIsInstance(error, ValueError)
        self.assertEqual(writes, [])

        def moved(count, registry):
            if count == 3:
                registry['main'] = DIGEST
        error, receipt, writes, _ = self.exercise(True, source_hook=moved)
        self.assertIsInstance(error, ValueError)
        self.assertEqual(len(writes), 1)
        self.assertEqual(receipt['state'], 'immutable_verified')

    def test_uncertain_write_and_cancellation_never_retry(self):
        for target in ('sha-' + SOURCE, 'main'):
            for exception in (TimeoutError, KeyboardInterrupt):
                with self.subTest(target=target, exception=exception):
                    def uncertain(tag, registry):
                        registry[tag] = DIGEST
                        if tag == target:
                            raise exception('outcome uncertain')
                    error, receipt, writes, _ = self.exercise(True, copy_hook=uncertain)
                    self.assertIsInstance(error, exception)
                    self.assertEqual([tag for tag, _ in writes].count(target), 1)
                    self.assertEqual(receipt['state'], 'main_write_attempted' if target == 'main'
                                     else 'immutable_write_attempted')
                    self.assertEqual(receipt['terminal'], 'failed_or_interrupted')

    def test_wrong_postwrite_digest_stops(self):
        for target in ('sha-' + SOURCE, 'main'):
            with self.subTest(target=target):
                def corrupt(tag, registry):
                    registry[tag] = OTHER if tag == target else DIGEST
                error, receipt, writes, _ = self.exercise(True, copy_hook=corrupt)
                self.assertIsInstance(error, ValueError)
                self.assertEqual(len(writes), 2 if target == 'main' else 1)
                self.assertNotEqual(receipt['state'], 'complete')


class WorkflowContract(unittest.TestCase):
    def setUp(self):
        self.root = Path(__file__).resolve().parents[1]
        self.qualify = (self.root / '.github/workflows/swarm-ghcr.yml').read_text()
        self.publish = (self.root / '.github/workflows/swarm-ghcr-publish.yml').read_text()

    def test_automatic_workflow_never_authenticates_or_publishes(self):
        self.assertIn('  push:', self.qualify)
        for path in ('.dockerignore', 'fuzz/**', '.github/workflows/swarm-ghcr-publish.yml'):
            self.assertIn(f'      - "{path}"', self.qualify)
        self.assertNotIn('pull_request:', self.qualify)
        self.assertNotRegex(self.qualify, r'packages:|login-action|GH_TOKEN|secrets\.|push: true')
        self.assertIn('          push: false', self.qualify)
        self.assertIn('outputs: type=oci,dest=', self.qualify)
        self.assertIn('python3 scripts/ci_swarm_image.py qualify', self.qualify)
        self.assertIn('            ${{ runner.temp }}/swarm-image/image.tar', self.qualify)
        self.assertIn('            ${{ runner.temp }}/swarm-image/qualification.json', self.qualify)
        self.assertIn('          if-no-files-found: error', self.qualify)
        self.assertIn('    timeout-minutes: 20', self.qualify)

    def test_publisher_only_dispatches_and_retains_failures(self):
        self.assertIn('  workflow_dispatch:', self.publish)
        self.assertNotRegex(self.publish, re.compile(r'^  (push|pull_request|workflow_run|schedule):', re.MULTILINE))
        self.assertIn('group: swarm-ghcr-publication', self.publish)
        self.assertIn('cancel-in-progress: false', self.publish)
        self.assertIn('run: test "$GITHUB_REF" = refs/heads/main', self.publish)
        self.assertNotRegex(self.publish, r'build-push-action|setup-buildx-action|docker build')
        self.assertIn('      packages: write', self.publish)
        self.assertIn('python3 scripts/ci_swarm_image.py publish', self.publish)
        self.assertIn('        if: always()', self.publish)
        self.assertIn('path: ${{ runner.temp }}/swarm-publication/publication.json', self.publish)
        self.assertIn('          if-no-files-found: error', self.publish)
        self.assertIn('Advisory public swarm GHCR manifest visibility', self.publish)

    def test_receipt_survives_invalid_request_before_any_external_effect(self):
        with tempfile.TemporaryDirectory() as directory:
            bad = request()
            bad['unknown'] = 'ignored field must refuse'
            with patch.object(image, 'api') as api_call, patch.object(image, 'registry_token') as token:
                with self.assertRaisesRegex(ValueError, 'fields'):
                    image.publish(directory, bad, context())
                api_call.assert_not_called()
                token.assert_not_called()
            receipt = json.loads((Path(directory) / 'publication.json').read_text())
            self.assertEqual(receipt['state'], 'validating_request')
            self.assertFalse(receipt['registry_write_attempted'])
            self.assertEqual(receipt['terminal'], 'failed_or_interrupted')

    def test_invoked_publisher_normalizes_omitted_optional_input(self):
        req = request()
        del req['expected_main_digest']
        with tempfile.TemporaryDirectory() as directory, \
             patch.object(image, 'source_identity', return_value=(SOURCE, TREE)), \
             patch.object(image, 'current_source', side_effect=TimeoutError('API unavailable')) as check, \
             patch.object(image, 'registry_token') as token:
            with self.assertRaises(TimeoutError):
                image.publish(directory, req, context())
            self.assertEqual(check.call_args.args[0]['expected_main_digest'], '')
            token.assert_not_called()
            receipt = json.loads((Path(directory) / 'publication.json').read_text())
            self.assertEqual(receipt['state'], 'validating_source')
            self.assertEqual(receipt['request']['expected_main_digest'], '')
            self.assertFalse(receipt['registry_write_attempted'])

    def test_preflight_failure_and_upload_contract_cannot_report_success(self):
        with tempfile.TemporaryDirectory() as directory:
            with patch.object(image, 'source_identity', return_value=(SOURCE, TREE)), \
                 patch.object(image, 'current_source', side_effect=TimeoutError('API unavailable')):
                with self.assertRaises(TimeoutError):
                    image.publish(directory, request(), context())
            receipt = json.loads((Path(directory) / 'publication.json').read_text())
            self.assertEqual(receipt['terminal'], 'failed_or_interrupted')
            self.assertNotEqual(receipt['state'], 'complete')
        upload = self.publish.split('      - name: Retain publication checkpoint', 1)[1]
        self.assertNotIn('continue-on-error', upload)

    def test_daemon_roundtrip_binds_classic_and_containerd_ids(self):
        version = image.tomllib.loads((self.root / 'Cargo.toml').read_text())['workspace']['package']['version']
        for daemon_kind in ('classic', 'containerd'):
            for substitute, cleanup_fails in ((False, False), (True, False), (False, True)):
                with self.subTest(daemon=daemon_kind, substitute=substitute, cleanup=cleanup_fails), tempfile.TemporaryDirectory() as directory:
                    path = Path(directory) / 'image.tar'
                    archive(path, oci_members())
                    identity = image.inspect_oci(path)
                    actual_id = identity['config_digest'] if daemon_kind == 'classic' else OTHER
                    calls = []
                    def execute(args, **kwargs):
                        calls.append(args)
                        if args[:3] == ['docker', 'image', 'inspect']:
                            return actual_id.encode()
                        if args[:2] == ['skopeo', 'copy'] and args[2].startswith('docker-daemon:'):
                            self.assertEqual(args[2], 'docker-daemon:' + actual_id)
                            members = oci_members(revision='e' * 40 if substitute else SOURCE, compressed=True)
                            archive(Path(args[3].removeprefix('oci-archive:')), members)
                        if args[:3] == ['docker', 'image', 'rm'] and cleanup_fails:
                            raise ValueError('cleanup failed after successful smoke')
                        if args[:2] == ['docker', 'run']:
                            self.assertIn(actual_id, args)
                            return (f'tokmd {version}\n' if args[-1] == '--version' else 'Usage: tokmd\n').encode()
                        return b''
                    with patch.object(image, 'source_identity', return_value=(SOURCE, TREE)), \
                         patch.object(image, 'command', side_effect=execute):
                        if substitute or cleanup_fails:
                            with self.assertRaises(ValueError):
                                image.qualify(directory, context('push'))
                            self.assertFalse((Path(directory) / 'qualification.json').exists())
                            self.assertEqual(any(call[:2] == ['docker', 'run'] for call in calls), cleanup_fails)
                        else:
                            receipt = image.qualify(directory, context('push'))
                            self.assertEqual(receipt['daemon_image_id'], actual_id)
                            self.assertEqual(receipt['config_digest'], identity['config_digest'])
                            self.assertEqual(receipt['layer_diff_ids'], identity['layer_diff_ids'])
                            self.assertTrue(receipt['loaded_identity_verified'])
                            self.assertFalse(receipt['registry_write'])
                    self.assertEqual(calls[-1][:3], ['docker', 'image', 'rm'])

    def test_cleanup_preserves_primary_failure_and_never_qualifies(self):
        for import_fails in (False, True):
            with self.subTest(import_fails=import_fails), tempfile.TemporaryDirectory() as directory:
                archive(Path(directory) / 'image.tar', oci_members())
                calls = []
                def execute(args, **kwargs):
                    calls.append(args)
                    if args[:2] == ['skopeo', 'copy'] and import_fails:
                        raise ValueError('original import failure')
                    if args[:3] == ['docker', 'image', 'inspect']:
                        raise ValueError('original inspection failure')
                    if args[:3] == ['docker', 'image', 'rm']:
                        raise ValueError('secondary cleanup failure')
                    return b''
                with patch.object(image, 'source_identity', return_value=(SOURCE, TREE)), \
                     patch.object(image, 'command', side_effect=execute):
                    with self.assertRaisesRegex(ValueError, 'original'):
                        image.qualify(directory, context('push'))
                self.assertFalse((Path(directory) / 'qualification.json').exists())
                self.assertEqual(any(c[:3] == ['docker', 'image', 'rm'] for c in calls), not import_fails)


if __name__ == '__main__':
    unittest.main()
