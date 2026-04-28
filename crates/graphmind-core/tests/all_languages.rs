use graphmind_core::parse;

#[test]
fn parse_c() {
    let source = r#"
#include <stdio.h>
struct Point { int x; int y; };
void greet(const char* name) { printf("Hello"); }
int main() { greet("world"); return 0; }
"#;
    let r = parse("test.c", source, "c").unwrap();
    assert!(!r.symbols.is_empty(), "C: no symbols found");
    let names: Vec<&str> = r.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.iter().any(|n| n.contains("greet") || n.contains("main")), "C: missing functions, got: {:?}", names);
    assert!(!r.imports.is_empty(), "C: no imports found");
}

#[test]
fn parse_objc() {
    let source = r#"
#import <Foundation/Foundation.h>
@interface Greeter : NSObject
- (void)sayHello;
@end
@implementation Greeter
- (void)sayHello { [self doWork]; }
- (void)doWork {}
@end
"#;
    let r = parse("test.m", source, "objc").unwrap();
    assert!(!r.symbols.is_empty(), "ObjC: no symbols found");
    let names: Vec<&str> = r.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.iter().any(|n| n.contains("Greeter")), "ObjC: missing class, got: {:?}", names);
}

#[test]
fn parse_java() {
    let source = r#"
import java.util.List;
public class MyService {
    public void process(String input) { helper(input); }
    private void helper(String s) {}
}
"#;
    let r = parse("Test.java", source, "java").unwrap();
    assert!(!r.symbols.is_empty(), "Java: no symbols found");
    let names: Vec<&str> = r.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"MyService"), "Java: missing class, got: {:?}", names);
    assert!(names.contains(&"process"), "Java: missing method, got: {:?}", names);
    assert!(!r.imports.is_empty(), "Java: no imports found");
}

#[test]
fn parse_php() {
    let source = r#"<?php
use App\Models\User;
class UserController {
    public function index() { return $this->getUsers(); }
    private function getUsers() {}
}
"#;
    let r = parse("test.php", source, "php").unwrap();
    assert!(!r.symbols.is_empty(), "PHP: no symbols found");
    let names: Vec<&str> = r.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"UserController"), "PHP: missing class, got: {:?}", names);
}

#[test]
fn parse_swift() {
    let source = r#"
import Foundation
class NetworkClient {
    func fetchData(url: String) { parse(url) }
    func parse(_ data: String) {}
}
protocol Fetchable { func fetch() }
"#;
    let r = parse("test.swift", source, "swift").unwrap();
    assert!(!r.symbols.is_empty(), "Swift: no symbols found");
    let names: Vec<&str> = r.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"NetworkClient"), "Swift: missing class, got: {:?}", names);
    assert!(names.contains(&"Fetchable"), "Swift: missing protocol, got: {:?}", names);
    assert!(!r.imports.is_empty(), "Swift: no imports found");
}

#[test]
fn parse_bash() {
    let source = r#"#!/bin/bash
deploy() { build; echo "deploying"; }
build() { echo "building"; }
"#;
    let r = parse("test.sh", source, "bash").unwrap();
    assert!(!r.symbols.is_empty(), "Bash: no symbols found");
    let names: Vec<&str> = r.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"deploy"), "Bash: missing function, got: {:?}", names);
    assert!(names.contains(&"build"), "Bash: missing function, got: {:?}", names);
}

#[test]
fn parse_perl() {
    let source = r#"
use strict;
use warnings;
sub process { my ($self) = @_; }
sub helper { }
"#;
    let r = parse("test.pl", source, "perl").unwrap();
    assert!(!r.symbols.is_empty(), "Perl: no symbols found, parsing succeeded but no extraction");
    assert!(!r.imports.is_empty(), "Perl: no imports found");
}

#[test]
fn parse_css() {
    let source = r#"
@import "reset.css";
.container { display: flex; }
#main { color: red; }
"#;
    let r = parse("test.css", source, "css").unwrap();
    assert!(!r.imports.is_empty(), "CSS: no imports found");
}

#[test]
fn parse_scss() {
    let source = r#"
@import "variables";
@mixin flex-center { display: flex; }
.card { border: 1px solid; }
"#;
    let r = parse("test.scss", source, "scss").unwrap();
    assert!(!r.symbols.is_empty(), "SCSS: no symbols found");
}

#[test]
fn parse_html() {
    let source = r#"<html>
<head><script src="app.js"></script></head>
<body><div id="root"></div></body>
</html>"#;
    let r = parse("test.html", source, "html").unwrap();
    // HTML parsing should at minimum not error
    let _ = &r; // parse succeeded
}

#[test]
fn parse_toml() {
    let source = r#"
[package]
name = "myapp"

[dependencies]
serde = "1.0"
"#;
    let r = parse("test.toml", source, "toml").unwrap();
    // TOML should parse without error
    let _ = &r; // parse succeeded
}

#[test]
fn parse_dockerfile() {
    let source = r#"FROM node:20 AS builder
LABEL maintainer="dev@example.com"
RUN npm install
FROM alpine:3.19
COPY --from=builder /app /app
"#;
    let r = parse("Dockerfile", source, "dockerfile").unwrap();
    assert!(!r.symbols.is_empty(), "Dockerfile: no symbols found");
}

#[test]
fn parse_sql() {
    let source = r#"
CREATE TABLE users (id INT PRIMARY KEY, name VARCHAR(100));
CREATE VIEW active_users AS SELECT * FROM users WHERE active = 1;
"#;
    let r = parse("test.sql", source, "sql").unwrap();
    let _ = &r; // parse succeeded
}

#[test]
fn parse_cpp() {
    let source = r#"
#include <iostream>
namespace MyApp {
class Service {
public:
    void run() { helper(); }
private:
    void helper() {}
};
}
"#;
    let r = parse("test.cpp", source, "cpp").unwrap();
    assert!(!r.symbols.is_empty(), "C++: no symbols found");
    assert!(!r.imports.is_empty(), "C++: no imports found");
}

#[test]
fn parse_csharp() {
    let source = r#"
using System.Collections.Generic;
namespace MyApp {
    public class UserService {
        public void GetUsers() { Helper(); }
        private void Helper() {}
    }
    public interface IService { void Run(); }
}
"#;
    let r = parse("test.cs", source, "csharp").unwrap();
    assert!(!r.symbols.is_empty(), "C#: no symbols found");
    let names: Vec<&str> = r.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"UserService"), "C#: missing class, got: {:?}", names);
    assert!(!r.imports.is_empty(), "C#: no imports found");
}

#[test]
fn parse_kotlin() {
    let source = r#"
import kotlin.collections.List
class UserRepository {
    fun findAll(): List<String> { return fetch() }
    private fun fetch(): List<String> { return emptyList() }
}
interface Repository { fun count(): Int }
"#;
    let r = parse("test.kt", source, "kotlin").unwrap();
    assert!(!r.symbols.is_empty(), "Kotlin: no symbols found");
}

#[test]
fn parse_dart() {
    let source = r#"
import 'package:flutter/material.dart';
class MyWidget extends StatelessWidget {
    Widget build(BuildContext context) { return render(); }
    Widget render() { return Container(); }
}
"#;
    let r = parse("test.dart", source, "dart").unwrap();
    assert!(!r.symbols.is_empty(), "Dart: no symbols found");
}

#[test]
fn parse_scala() {
    let source = r#"
import scala.collection.mutable
class UserService {
    def findUser(id: Int): String = helper(id)
    private def helper(id: Int): String = ""
}
trait Repository { def count(): Int }
object Main { def main(args: Array[String]): Unit = {} }
"#;
    let r = parse("test.scala", source, "scala").unwrap();
    assert!(!r.symbols.is_empty(), "Scala: no symbols found");
}

#[test]
fn parse_r() {
    let source = r#"
library(ggplot2)
process_data <- function(x) { clean(x) }
clean <- function(x) { x }
"#;
    let r = parse("test.R", source, "r").unwrap();
    assert!(!r.symbols.is_empty(), "R: no symbols found");
    assert!(!r.imports.is_empty(), "R: no imports found");
}

#[test]
fn parse_graphql() {
    let source = r#"
type User {
    id: ID!
    name: String!
}
interface Node { id: ID! }
enum Role { ADMIN USER }
query GetUser($id: ID!) { user(id: $id) { name } }
"#;
    let r = parse("schema.graphql", source, "graphql").unwrap();
    assert!(!r.symbols.is_empty(), "GraphQL: no symbols found");
    let names: Vec<&str> = r.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"User"), "GraphQL: missing type, got: {:?}", names);
}

#[test]
fn parse_powershell() {
    let source = r#"
function Deploy-App {
    Build-Project
    Write-Host "Deploying"
}
function Build-Project { Write-Host "Building" }
"#;
    let r = parse("deploy.ps1", source, "powershell").unwrap();
    assert!(!r.symbols.is_empty(), "PowerShell: no symbols found");
}
