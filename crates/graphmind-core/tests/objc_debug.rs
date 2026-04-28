#[test]
fn debug_objc_tree() {
    let source = r#"#import <Foundation/Foundation.h>
@interface Greeter : NSObject
- (void)sayHello;
@end
@implementation Greeter
- (void)sayHello { [self doWork]; }
- (void)doWork {}
@end
"#;
    let mut parser = tree_sitter::Parser::new();
    let lang: tree_sitter::Language = tree_sitter_objc::LANGUAGE.into();
    parser.set_language(&lang).unwrap();
    let tree = parser.parse(source, None).unwrap();
    print_node(tree.root_node(), source, 0);
}

fn print_node(node: tree_sitter::Node, source: &str, depth: usize) {
    if node.is_named() && depth < 4 {
        let text = &source[node.start_byte()..std::cmp::min(node.end_byte(), node.start_byte() + 50)];
        let short = text.replace('\n', "\\n");
        println!("{}{} [field={:?}]  {:?}", "  ".repeat(depth), node.kind(), node.parent().and_then(|p| p.field_name_for_child(node.id() as u32)), short);
    }
    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i) {
            print_node(child, source, depth + 1);
        }
    }
}
