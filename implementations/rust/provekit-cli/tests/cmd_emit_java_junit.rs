// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn provekit_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_provekit"))
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn build_java_junit_emitter() -> PathBuf {
    let java_root = repo_root().join("implementations").join("java");
    let built = Command::new("mvn")
        .current_dir(&java_root)
        .args([
            "-q",
            "-pl",
            "provekit-emit-java-junit",
            "-am",
            "-DskipTests",
            "package",
        ])
        .output()
        .expect("spawn mvn package");
    assert!(
        built.status.success(),
        "mvn package provekit-emit-java-junit failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&built.stdout),
        String::from_utf8_lossy(&built.stderr)
    );
    let jar = java_root
        .join("provekit-emit-java-junit")
        .join("target")
        .join("provekit-emit-java-junit.jar");
    assert!(jar.exists(), "missing emitter jar {}", jar.display());
    jar
}

fn maven_java_bin() -> Option<PathBuf> {
    let output = Command::new("mvn").arg("-version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let java_home = text
        .lines()
        .find_map(|line| {
            line.strip_prefix("Java home: ")
                .or_else(|| line.split("runtime: ").nth(1))
        })
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let java_bin = PathBuf::from(java_home).join("bin").join("java");
    java_bin.exists().then_some(java_bin)
}

fn install_emit_registration(project: &Path, java_bin: &Path, jar: &Path) {
    let provekit_dir = project.join(".provekit");
    fs::create_dir_all(&provekit_dir).expect("mkdir .provekit");
    fs::write(
        provekit_dir.join("config.toml"),
        "exam_manifest_cid = \"blake3-512:00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000\"\n\
         \n\
         [[plugins]]\n\
         name = \"java-junit5\"\n\
         surface = \"java-junit5\"\n\
         emit = \"junit5\"\n",
    )
    .expect("write project config");

    let manifest = project
        .join(".provekit")
        .join("emit")
        .join("java-junit5")
        .join("manifest.toml");
    fs::create_dir_all(manifest.parent().unwrap()).expect("mkdir manifest");
    fs::write(
        manifest,
        format!(
            "name = \"java-junit5\"\ncommand = [\"{}\", \"-jar\", \"{}\", \"--rpc\"]\nworking_dir = \".\"\nprotocol_versions = [\"pep/1.7.0\"]\n",
            java_bin
                .display()
                .to_string()
                .replace('\\', "\\\\")
                .replace('"', "\\\""),
            jar.display()
                .to_string()
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
        ),
    )
    .expect("write emit manifest");
}

fn write_maven_test_project(out_dir: &Path) {
    fs::create_dir_all(out_dir.join("src/test/java")).expect("mkdir java test tree");
    fs::write(
        out_dir.join("pom.xml"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0 https://maven.apache.org/xsd/maven-4.0.0.xsd">
  <modelVersion>4.0.0</modelVersion>
  <groupId>example</groupId>
  <artifactId>provekit-java-emit-check</artifactId>
  <version>0.1.0</version>
  <properties>
    <maven.compiler.source>17</maven.compiler.source>
    <maven.compiler.target>17</maven.compiler.target>
    <project.build.sourceEncoding>UTF-8</project.build.sourceEncoding>
  </properties>
  <dependencies>
    <dependency>
      <groupId>org.junit.jupiter</groupId>
      <artifactId>junit-jupiter</artifactId>
      <version>5.10.0</version>
      <scope>test</scope>
    </dependency>
  </dependencies>
  <build>
    <plugins>
      <plugin>
        <groupId>org.apache.maven.plugins</groupId>
        <artifactId>maven-surefire-plugin</artifactId>
        <version>3.5.4</version>
      </plugin>
    </plugins>
  </build>
</project>
"#,
    )
    .expect("write pom");
}

#[test]
fn emit_java_junit_dispatches_real_emitter_and_maven_checks_output() {
    let Some(java_bin) = maven_java_bin() else {
        eprintln!("skipping: mvn is unavailable or did not report a usable Java home");
        return;
    };

    let temp = tempfile::tempdir().expect("tempdir");
    let project = temp.path().join("project");
    let out_dir = temp.path().join("out");
    fs::create_dir_all(&project).expect("mkdir project");
    write_maven_test_project(&out_dir);

    let jar = build_java_junit_emitter();
    install_emit_registration(&project, &java_bin, &jar);

    let plan = project.join("plan.json");
    fs::write(
        &plan,
        serde_json::to_vec_pretty(&serde_json::json!({
            "contract_id": "concept:eq",
            "function": "identity",
            "params": ["a", "b"],
            "param_types": ["int", "int"],
            "predicates": [{
                "kind": "atomic",
                "name": "concept:eq",
                "args": [
                    {"kind": "const", "value": 2, "sort": {"kind": "primitive", "name": "Int"}},
                    {"kind": "const", "value": 2, "sort": {"kind": "primitive", "name": "Int"}}
                ]
            }]
        }))
        .expect("encode plan"),
    )
    .expect("write plan");

    let output = Command::new(provekit_bin())
        .arg("emit")
        .arg("--project")
        .arg(&project)
        .arg("--target")
        .arg("java")
        .arg("--framework")
        .arg("junit5")
        .arg("--plan")
        .arg(&plan)
        .arg("--out-dir")
        .arg(out_dir.join("src/test/java"))
        .arg("--compile-check")
        .arg("--json")
        .output()
        .expect("spawn provekit emit java");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "provekit emit java failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let receipt: Value = serde_json::from_str(&stdout).expect("emit stdout is JSON");
    assert_eq!(receipt["ok"], true, "receipt: {receipt}");
    assert_eq!(receipt["targetLanguage"], "java", "receipt: {receipt}");
    assert_eq!(receipt["targetFramework"], "junit5", "receipt: {receipt}");

    let emitted_path = out_dir.join("src/test/java/IdentityContractTest.java");
    let emitted = fs::read_to_string(&emitted_path)
        .unwrap_or_else(|_| panic!("read emitted {}", emitted_path.display()));
    assert!(
        emitted.contains("import org.junit.jupiter.api.Test;"),
        "emitted:\n{emitted}"
    );
    assert!(
        emitted.contains("assertEquals(2, 2);"),
        "emitted:\n{emitted}"
    );
}
