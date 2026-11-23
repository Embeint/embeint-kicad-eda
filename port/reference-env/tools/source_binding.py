#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Bind qualification to a clean exact Git export; verify snapshots without Git.

Run export with the checkout's native Git (Windows Git for Windows worktrees).
The printed manifest digest is an explicit input to qualification. Original
reference-stage provenance is retained byte for byte, never repaired/relabelled.
"""
import argparse
import ast
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import sys
import tarfile
import tomllib


def require(test, message):
    if not test: raise ValueError(message)


def sha(data): return hashlib.sha256(data).hexdigest()


def read_json(path):
    def pairs(items):
        result={}
        for k,v in items:
            require(k not in result, 'duplicate metadata key'); result[k]=v
        return result
    return json.loads(path.read_text(encoding='utf-8'), object_pairs_hook=pairs)


def git(repo, *args):
    # Never use WSL Git on Windows metadata, including linked worktree files.
    require(not (sys.platform != 'win32' and str(repo.resolve()).startswith('/mnt/')), 'Use Windows Git to export this checkout')
    return subprocess.check_output(['git','-C',str(repo),*args], stderr=subprocess.PIPE)


def archive_files(path):
    files={}
    with tarfile.open(path) as archive:
        for member in archive:
            p=PurePosixPath(member.name)
            require(not p.is_absolute() and '..' not in p.parts and str(p)==member.name.rstrip('/'), 'unsafe source archive path')
            if member.isdir(): continue
            require(member.name not in files, 'duplicate archive entry')
            if member.isfile():
                files[member.name]={'kind':'file','sha256':sha(archive.extractfile(member).read())}
            elif member.issym():
                require(not PurePosixPath(member.linkname).is_absolute(), 'absolute source link')
                # Normalize the target lexically, preventing escapes and link subtrees.
                parts=list(p.parent.parts)
                for part in PurePosixPath(member.linkname).parts:
                    if part=='..': require(bool(parts),'escaping source link');parts.pop()
                    elif part!='.':parts.append(part)
                files[member.name]={'kind':'symlink','target':member.linkname}
            else: raise ValueError('unexpected source archive member')
    for name,record in files.items():
        require(not any(str(parent) in files for parent in PurePosixPath(name).parents if str(parent)!='.'), 'file/link archive subtree')
    return dict(sorted(files.items()))


def check_tree(root, files, *, checkout=False):
    for name,record in files.items():
        path=root/name
        if record['kind']=='symlink':
            require(path.is_symlink() and os.readlink(path)==record['target'], 'source link drift: '+name)
        else:
            require(path.is_file() and not path.is_symlink(), 'missing/non-file source: '+name)
            data=path.read_bytes()
            # Windows text checkout conversion is not source drift. The build
            # snapshot always uses canonical Git bytes; binary data is exact.
            actual=sha(data)
            if checkout and actual!=record['sha256'] and b'\0' not in data:
                actual=sha(data.replace(b'\r\n',b'\n'))
            require(actual==record['sha256'], 'actual source bytes drift: '+name)
    if not checkout:
        allowed=('.port-deps/','.port-venv/','.west/','rust/target/')
        for path in root.rglob('*'):
            if path.is_dir():continue
            name=path.relative_to(root).as_posix()
            require(name in files or name.startswith(allowed),
                    'unexpected source snapshot file: '+name)


def clean_checkout(repo, revision, archive):
    require(re.fullmatch('[0-9a-f]{40}',revision), 'full intended revision required')
    require(git(repo,'rev-parse','HEAD').decode().strip()==revision,'checkout revision differs from intended head')
    require(not git(repo,'status','--porcelain','--untracked-files=all').strip(),'source checkout is dirty/untracked')
    git(repo,'archive','--format=tar','--output='+str(archive),revision)
    files=archive_files(archive)
    check_tree(repo,files,checkout=True)
    return files


def stage_check(stage, proof):
    manifest=read_json(stage/'source-manifest.json')
    require(manifest['kicad_pin']==proof['pin'] and manifest['overlay_revision']==proof['revision']
            and manifest['overlay_dirty_at_export'] is False,'stale/dirty/mixed stage provenance')
    require(manifest['files']==proof['reference_files'],'stage source identities differ from actual pinned Git bytes')
    require(sha((stage/'reference.tar').read_bytes())==manifest['archive_sha256'],'stage archive hash differs')
    actual=archive_files(stage/'reference.tar')
    require(all(v['kind']=='file' for v in actual.values()) and
            {k:v['sha256'] for k,v in actual.items()}==proof['reference_files'],'stage actual source drift')


def export(repo, revision, output, stage=None):
    output.mkdir(parents=True,exist_ok=False)
    source=output/'source.tar'
    files=clean_checkout(repo,revision,source)
    # Read constants from the exact committed overlay, not imported mutable code.
    pin_source=git(repo,'show',revision+':port/oracle/pin.py').decode()
    constants={node.targets[0].id:ast.literal_eval(node.value) for node in ast.parse(pin_source).body
               if isinstance(node,ast.Assign) and isinstance(node.targets[0],ast.Name)}
    pin=constants['KICAD_PIN']
    pins=tomllib.loads(git(repo,'show',revision+':port/pins.toml').decode())
    require(pin==pins['kicad']['revision'],'source pin interfaces differ')
    reference=output/'expected-reference.tar'
    git(repo,'archive','--format=tar','--output='+str(reference),pin,*constants['SOURCE_PATHS'])
    expected=archive_files(reference)
    proof={'schema':1,'revision':revision,'tree':git(repo,'rev-parse',revision+'^{tree}').decode().strip(),
           'files':files,'source_tar_sha256':sha(source.read_bytes()),'pin':pin,
           'reference_files':{k:v['sha256'] for k,v in expected.items()},
           'source_clean':True,'export_platform':sys.platform,'git_version':git(repo,'--version').decode().strip()}
    target=output/'stage';target.mkdir()
    if stage is None:
        shutil.copyfile(reference,target/'reference.tar')
        manifest={'kicad_pin':pin,'files':proof['reference_files'],'overlay_revision':revision,
                  'overlay_dirty_at_export':False,'archive_sha256':sha(reference.read_bytes())}
        (target/'source-manifest.json').write_text(json.dumps(manifest,indent=2,sort_keys=True)+'\n')
    else:
        for name in ('reference.tar','source-manifest.json'):shutil.copyfile(stage/name,target/name)
    stage_check(target,proof)
    proof['stage_manifest_sha256']=sha((target/'source-manifest.json').read_bytes())
    proof['stage_tar_sha256']=sha((target/'reference.tar').read_bytes())
    reference.unlink()
    # Recheck the live source after export; the archive, not a live mount, is built.
    check_tree(repo,files,checkout=True)
    require(not git(repo,'status','--porcelain','--untracked-files=all').strip(),'source changed during export')
    path=output/'source.json';path.write_text(json.dumps(proof,indent=2,sort_keys=True)+'\n')
    return sha(path.read_bytes())


def proof_at(bundle, revision, digest):
    require(re.fullmatch('[0-9a-f]{40}',revision) and re.fullmatch('[0-9a-f]{64}',digest),'intended head and export digest required')
    require(sha((bundle/'source.json').read_bytes())==digest,'source export digest differs')
    proof=read_json(bundle/'source.json')
    require(type(proof['schema']) is int and proof['schema']==1 and proof['revision']==revision,'wrong source export revision/schema')
    require(sha((bundle/'stage/source-manifest.json').read_bytes())==proof['stage_manifest_sha256'] and
            sha((bundle/'stage/reference.tar').read_bytes())==proof['stage_tar_sha256'],'original stage bytes changed')
    stage_check(bundle/'stage',proof)
    return proof


def materialize(bundle, revision, digest, destination):
    proof=proof_at(bundle,revision,digest)
    archive=bundle/'source.tar'
    require(sha(archive.read_bytes())==proof['source_tar_sha256'] and archive_files(archive)==proof['files'],'source archive differs')
    destination.mkdir(parents=True,exist_ok=False)
    source=destination/'source';source.mkdir()
    # Safe membership/link checks above precede extraction. Links are created last.
    with tarfile.open(archive) as entries:
        for entry in entries:
            if entry.isdir() or entry.issym():continue
            path=source/entry.name;path.parent.mkdir(parents=True,exist_ok=True)
            path.write_bytes(entries.extractfile(entry).read());path.chmod(entry.mode)
    for name,record in proof['files'].items():
        if record['kind']=='symlink':
            path=source/name;path.parent.mkdir(parents=True,exist_ok=True);path.symlink_to(record['target'])
    shutil.copyfile(bundle/'source.json',destination/'source.json')
    shutil.copytree(bundle/'stage',destination/'stage')
    check_tree(source,proof['files'])


def check_snapshot(snapshot, revision, digest):
    proof=proof_at(snapshot,revision,digest)
    check_tree(snapshot/'source',proof['files'])
    return proof


def context_files(source, context, *, copy=False):
    names=['Dockerfile','candidate.toml','inputs.lock.json']
    for top in ('image','probe','tools'):
        names.extend(p.relative_to(source).as_posix() for p in (source/top).rglob('*')
                     if p.is_file() and '__pycache__' not in p.parts)
    expected={name:sha((source/name).read_bytes()) for name in names}
    if copy:
        context.mkdir(parents=True,exist_ok=True)
        for name in names:
            path=context/name;path.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(source/name,path)
    actual={p.relative_to(context).as_posix():sha(p.read_bytes()) for p in context.rglob('*')
            if p.is_file() and not p.relative_to(context).as_posix().startswith('inputs/')}
    require(actual==expected,'build context tools/source differ from intended snapshot')


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('action',choices=['export','materialize','check','checkout','context'])
    p.add_argument('--repo',type=Path);p.add_argument('--revision',required=True)
    p.add_argument('--out',type=Path);p.add_argument('--stage',type=Path)
    p.add_argument('--bundle',type=Path);p.add_argument('--source-sha256')
    args=p.parse_args()
    if args.action=='export': print(export(args.repo.resolve(),args.revision,args.out.resolve(),args.stage))
    elif args.action=='materialize':materialize(args.bundle,args.revision,args.source_sha256,args.out)
    elif args.action=='check':check_snapshot(args.bundle,args.revision,args.source_sha256)
    elif args.action=='context':
        check_snapshot(args.bundle,args.revision,args.source_sha256)
        context_files(args.bundle/'source/port/reference-env',args.out,copy=args.repo is not None)
    else:
        import tempfile
        with tempfile.TemporaryDirectory() as temp:
            clean_checkout(args.repo.resolve(),args.revision,Path(temp)/'source.tar')


if __name__=='__main__':
    try:main()
    except (ValueError,KeyError,OSError,subprocess.CalledProcessError) as error:
        print('Source binding failed: '+str(error),file=sys.stderr);sys.exit(1)
