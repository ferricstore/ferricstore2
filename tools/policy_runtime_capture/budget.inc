/* Rejecting Stage A only; no registry, resource or permit is instantiated. */
static ERL_NIF_TERM budget_atoms[24];

static int budget_load(ErlNifEnv *env, void **private_data) {
    static const char *const names[24] = {
        "ok", "unsupported_budget_protocol", "session_busy", "retry_registry",
        "closed", "unmanaged", "stale", "exhausted", "terminal", "reserve",
        "begin", "cancel", "settle", "abandon", "close", "control", "offer",
        "keeper", "ready", "commit", "retry", "cancelled", "issued", "budget_closed"
    };
    (void)private_data;
    for (unsigned i = 0; i < 24; i++) budget_atoms[i] = enif_make_atom(env, names[i]);
    return 0;
}

static void budget_unload(ErlNifEnv *env, void *private_data) {
    (void)env;
    (void)private_data;
}

static ERL_NIF_TERM budget_register(ErlNifEnv *env, int argc, const ERL_NIF_TERM argv[]) {
    (void)env;
    (void)argc;
    (void)argv;
    return budget_atoms[1];
}

static ERL_NIF_TERM budget_enter(ErlNifEnv *env, int argc, const ERL_NIF_TERM argv[]) {
    (void)env;
    (void)argc;
    (void)argv;
    return budget_atoms[1];
}

static ERL_NIF_TERM budget_bind(ErlNifEnv *env, int argc, const ERL_NIF_TERM argv[]) {
    (void)env;
    (void)argc;
    (void)argv;
    return budget_atoms[1];
}

static ERL_NIF_TERM budget_read(ErlNifEnv *env, int argc, const ERL_NIF_TERM argv[]) {
    (void)env;
    (void)argc;
    (void)argv;
    return budget_atoms[1];
}

static ERL_NIF_TERM budget_control(ErlNifEnv *env, int argc, const ERL_NIF_TERM argv[]) {
    (void)env;
    (void)argc;
    (void)argv;
    return budget_atoms[1];
}
