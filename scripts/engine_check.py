"""Run the engine fixture through Godot, Unity and Unreal and record the evidence.

    python scripts/engine_check.py [--godot godot] [--unity <Unity.exe>] [--unreal <UE root>]
                                   [--only godot,unity,unreal] [--evidence docs/evidence/engine-check.json]

Each engine gets a fresh project under target/engine-check. The fixture comes from
`cargo run --example create_engine_fixture`. Exit status is nonzero when any check
fails or a requested engine cannot run.
"""
import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WORK = ROOT / "target" / "engine-check"
FIXTURE = WORK / "fixture"


def run(cmd, log, timeout=1800, cwd=None):
    started = time.time()
    with open(log, "w", encoding="utf-8", errors="replace") as out:
        proc = subprocess.run(cmd, stdout=out, stderr=subprocess.STDOUT, timeout=timeout, cwd=cwd)
    return proc.returncode, time.time() - started


def lines(log, pattern):
    text = Path(log).read_text(encoding="utf-8", errors="replace")
    return [line for line in text.splitlines() if re.search(pattern, line)]


def summary(log):
    found = lines(log, r"SUMMARY \{")
    if not found:
        return None
    return json.loads(found[-1].split("SUMMARY ", 1)[1])


def fixture():
    if WORK.exists():
        shutil.rmtree(WORK)
    WORK.mkdir(parents=True)
    subprocess.run(["cargo", "run", "--quiet", "--example", "create_engine_fixture", "--", str(FIXTURE)],
                   cwd=ROOT, check=True)


def godot(exe):
    project = WORK / "godot"
    project.mkdir()
    for name in ("project.godot", "check.gd"):
        shutil.copy(ROOT / "integrations/verification/godot" / name, project / name)
    for package in ("main", "directions"):
        shutil.copytree(FIXTURE / "godot" / package, project / package)
    run([exe, "--headless", "--path", str(project), "--import"], WORK / "godot-import.log")
    runs = []
    for package in ("main", "directions"):
        for mode, flags in (("fixed 240 fps", ["--fixed-fps", "240"]), ("fixed 10 fps", ["--fixed-fps", "10"]),
                            ("real time", ["--max-fps", "240"])):
            log = WORK / f"godot-{package}-{mode.replace(' ', '-')}.log"
            args = [f"res://{package}/ReBonePlayer.gd", f"res://{package}/package.json"]
            if mode == "real time":
                args.append("realtime")
            code, seconds = run([exe, "--headless", *flags, "--path", str(project), "--script", "res://check.gd", "--", *args], log)
            result = summary(log) or {"failures": 1, "error": "no summary"}
            result.update(package=package, mode=mode, exit=code, seconds=round(seconds, 1),
                          failed=lines(log, r"CHECK .* FAIL")[:20])
            runs.append(result)
    return runs


def unity(exe):
    project = WORK / "unity"
    code, _ = run([exe, "-batchmode", "-nographics", "-quit", "-createProject", str(project)], WORK / "unity-create.log")
    if code != 0:
        return [{"failures": 1, "error": "Unity could not create a project (exit %d); see %s" % (code, "unity-create.log"),
                 "license": lines(WORK / "unity-create.log", r"license|License")[-3:]}]
    assets = project / "Assets"
    scripts = assets / "ReBoneBase"
    (scripts / "Editor").mkdir(parents=True)
    # One copy of the runtime scripts serves every package in a project.
    shutil.copy(FIXTURE / "unity/main/ReBonePlayer.cs", scripts / "ReBonePlayer.cs")
    shutil.copy(FIXTURE / "unity/main/Editor/ReBoneImporter.cs", scripts / "Editor/ReBoneImporter.cs")
    (assets / "Editor").mkdir()
    shutil.copy(ROOT / "integrations/verification/unity/Editor/ReBoneCheck.cs", assets / "Editor/ReBoneCheck.cs")
    for package in ("main", "directions"):
        shutil.copytree(FIXTURE / "unity" / package, assets / "Fixture" / package,
                        ignore=shutil.ignore_patterns("*.cs", "Editor"))
    report = WORK / "unity-report.txt"
    packages = ",".join(f"Assets/Fixture/{p}/package.json" for p in ("main", "directions"))
    log = WORK / "unity-check.log"
    code, seconds = run([exe, "-batchmode", "-nographics", "-projectPath", str(project), "-executeMethod", "ReBoneCheck.Run",
                         "-rebonePackages", packages, "-reboneReport", str(report), "-logFile", str(log)], WORK / "unity-run.log")
    result = summary(log) or {"failures": 1, "error": "no summary", "compile": lines(log, r"error CS")[:10]}
    result.update(exit=code, seconds=round(seconds, 1), failed=lines(log, r"CHECK .* FAIL")[:20])
    return [result]


def unreal(root):
    engine = Path(root)
    editor = engine / "Engine/Binaries/Win64/UnrealEditor-Cmd.exe"
    build = WORK / "ue-plugin"
    code, seconds = run([str(engine / "Engine/Build/BatchFiles/RunUAT.bat"), "BuildPlugin",
                         "-Plugin=" + str(FIXTURE / "unreal/main/Plugin/ReBoneBase/ReBoneBase.uplugin"),
                         "-Package=" + str(build), "-TargetPlatforms=Win64", "-Rocket"], WORK / "unreal-build.log", timeout=3600)
    if code != 0:
        return [{"failures": 1, "error": "plugin build failed", "detail": lines(WORK / "unreal-build.log", r"error")[:10]}]
    project = WORK / "ue"
    (project / "Plugins").mkdir(parents=True)
    shutil.copytree(build, project / "Plugins/ReBoneBase")
    (project / "Check.uproject").write_text(json.dumps({
        "FileVersion": 3, "EngineAssociation": "", "Plugins": [
            {"Name": n, "Enabled": True} for n in ("Paper2D", "PythonScriptPlugin", "EditorScriptingUtilities", "ReBoneBase")]}), encoding="utf-8")
    uproject = project.as_posix() + "/Check.uproject"
    common = ["-unattended", "-nopause", "-nullrhi", "-stdout", "-FullStdOutLogOutput"]
    results = []
    version = (engine / "Engine/Build/Build.version")
    build_info = json.loads(version.read_text()) if version.exists() else {}
    engine_name = "Unreal Engine %s.%s.%s" % (build_info.get("MajorVersion"), build_info.get("MinorVersion"), build_info.get("PatchVersion"))
    checks = []
    for package in ("main", "directions"):
        folder = (FIXTURE / "unreal" / package).as_posix()
        log = WORK / f"unreal-import-{package}.log"
        code, _ = run([str(editor), uproject, "-run=pythonscript", f"-script={folder}/ReBoneImport.py", *common], log)
        imported = lines(log, r"ReBoneBase imported")
        results.append({"step": "import " + package, "exit": code, "result": imported[-1].split("LogPython: ")[-1] if imported else None,
                        "failures": 0 if code == 0 and imported else 1})
        checks += [folder, "/Game/ReBoneBase/" + package]
    log = WORK / "unreal-verify.log"
    run([str(editor), uproject, "-run=pythonscript",
         "-script=" + " ".join([(ROOT / "integrations/unreal/verify_import.py").as_posix(), *checks]), *common], log)
    verified = lines(log, r"VERIFY ")
    results.append({"step": "asset comparison", "result": [v.split("LogPython: ")[-1] for v in verified],
                    "failures": sum(1 for v in verified if "VERIFY FAIL" in v or "PROBLEM" in v) + (0 if verified else 1)})
    log = WORK / "unreal-automation.log"
    run([str(editor), uproject, "-ExecCmds=Automation RunTests ReBoneBase;Quit", "-testexit=Automation Test Queue Empty", *common], log)
    done = lines(log, r"Test Completed")
    results.append({"step": "automation", "result": [d.split("Display: ")[-1] for d in done],
                    "failures": sum(1 for d in done if "Success" not in d) + (0 if done else 1),
                    "errors": lines(log, r"LogAutomationController: Error")[:20]})
    for r in results:
        r["engine"] = engine_name
    return results


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--godot", default=shutil.which("godot") or "godot")
    parser.add_argument("--unity", default=r"C:\Program Files\Unity\Hub\Editor\6000.4.6f1\Editor\Unity.exe")
    parser.add_argument("--unreal", default=r"C:\Program Files\Epic Games\UE_5.8")
    parser.add_argument("--only", default="godot,unity,unreal")
    parser.add_argument("--evidence", default=str(ROOT / "docs/evidence/engine-check.json"))
    args = parser.parse_args()
    fixture()
    engines = args.only.split(",")
    evidence = {"fixture": "examples/create_engine_fixture.rs", "date": time.strftime("%Y-%m-%d"), "results": {}}
    if "godot" in engines:
        evidence["results"]["godot"] = godot(args.godot)
    if "unity" in engines:
        evidence["results"]["unity"] = unity(args.unity)
    if "unreal" in engines:
        evidence["results"]["unreal"] = unreal(args.unreal)
    Path(args.evidence).parent.mkdir(parents=True, exist_ok=True)
    Path(args.evidence).write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
    failed = {name: sum(r.get("failures", 1) for r in runs) for name, runs in evidence["results"].items()}
    for name, count in failed.items():
        print(f"{name}: {'OK' if count == 0 else f'{count} failures'}")
    print("Evidence written to " + args.evidence)
    return 1 if any(failed.values()) else 0


if __name__ == "__main__":
    sys.exit(main())
