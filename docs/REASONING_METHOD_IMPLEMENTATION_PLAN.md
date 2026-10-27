# Executable reasoning implementation

The current implementation uses EAL-owned method contracts and portable
executable packages. EngArg authors and validates the argument, support
requirements and exact method queries; EAL collects tool observations and
executes those queries under the declared input bindings.

The maintained interfaces and acceptance evidence are described in:

- [Reasoning methods](REASONING_METHODS.md): canonical method descriptors,
  empirical inputs, result predicates and explicit argument revision.
- [Executable packages](executable-packages.md): direct TOML bindings, compilation,
  package identity and runtime execution.
- [Support planning](support-planning.md): authoring, bounded provider calls,
  unbound operations and retained dissent.
- [Finite inventory example](CATALOG_SYNC_AUTHORING.md): an actual MCP source,
  command audit and executable deductive conjunction without registration.

Runtime behavior is defined by the EAL source and operator tool manifest.
Capability discovery is optional authoring assistance. Historical execution
sidecars and duplicated EngArg method schemas are not runtime dependencies.
