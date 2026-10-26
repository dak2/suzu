# Suzu

A systems language with memory safety and no garbage collector. Borrows are second-class, so there are no lifetimes.

```
fn apply_discount(mut sales: Array[Sale], percent: Int) {
    for mut s in sales {
        s.price = s.price * (100 - percent) / 100
    }
}
```

Status: early design. Nothing runs yet.

- `spec/design.md`: core design
- `spec/grammar.ebnf`: grammar
- `examples/`: example programs (`.sz`)

```
cargo test
```
