static void budget_dtor(ErlNifEnv *env, void *data) {
    struct budget_registry *r = enif_priv_data(env);
    struct budget_session *s = data;
    if (s->request_env) enif_free_env(s->request_env);
    enif_mutex_lock(r->mutex);
    ASSERT(!(s->flags & (B_LINKED | B_CLEARING | B_BOUND | B_OWNER_MON | B_KEEPER_MON)));
    ASSERT(r->live_resources > 0);
    r->live_resources--;
    int destroy = r->flags && !r->root && !r->live_resources;
    enif_mutex_unlock(r->mutex);
    if (destroy) {
        enif_mutex_destroy(r->mutex);
        enif_free(r);
    }
}

static void budget_down(ErlNifEnv *env, void *data, ErlNifPid *pid, ErlNifMonitor *monitor) {
    struct budget_registry *r = enif_priv_data(env);
    struct budget_session *s = data;
    int drop = 0;
    enif_mutex_lock(r->mutex);
    if ((s->flags & B_KEEPER_MON) && !enif_compare_pids(pid, &s->node.key) &&
        !enif_compare_monitors(monitor, &s->keeper_monitor)) {
        s->flags &= ~B_KEEPER_MON;
        if (s->flags & B_LINKED) {
            budget_rbt_delete(&r->root, &s->node);
            s->flags &= ~B_LINKED;
            drop = 1;
        }
    } else if ((s->flags & B_OWNER_MON) && !enif_compare_pids(pid, &s->owner) &&
               !enif_compare_monitors(monitor, &s->owner_monitor)) {
        s->flags &= ~B_OWNER_MON;
    } else {
        enif_mutex_unlock(r->mutex);
        return;
    }
    if (s->phase != B_CLOSED) {
        s->spent_words += s->reserved_words;
        s->spent_pins += s->reserved_pins;
        s->reserved_words = 0;
        s->reserved_pins = 0;
        s->phase = B_CLOSED;
    }
    budget_terminate(env, s);
    if (drop) enif_release_resource(s); /* Registry ownership, outside the lock. */
}

static int budget_load(ErlNifEnv *env, void **private_data) {
    static const char *const names[24] = {
        "ok", "decoded_words", "session_busy", "retry_registry", "closed", "unmanaged",
        "stale", "pinned_bytes", "terminal", "reserve", "begin", "cancel", "settle",
        "abandon", "close", "control", "offer", "keeper", "ready", "commit", "retry",
        "cancelled", "issued", "budget_closed"
    };
    struct budget_registry *r = enif_alloc(sizeof(*r));
    if (!r) return -1;
    memset(r, 0, sizeof(*r));
    r->mutex = enif_mutex_create(NULL);
    if (!r->mutex) {
        enif_free(r);
        return -1;
    }
    for (unsigned i = 0; i < 24; i++) r->atoms[i] = enif_make_atom(env, names[i]);
    ErlNifResourceTypeInit callbacks = {.dtor = budget_dtor, .down = budget_down, .members = 3};
    r->type = enif_open_resource_type_x(env, "capture_budget", &callbacks, ERL_NIF_RT_CREATE, NULL);
    if (!r->type) {
        enif_mutex_destroy(r->mutex);
        enif_free(r);
        return -1;
    }
    *private_data = r;
    return 0;
}

static void budget_unload(ErlNifEnv *env, void *private_data) {
    (void)env;
    struct budget_registry *r = private_data;
    enif_mutex_lock(r->mutex);
    r->flags = 1;
    int destroy = !r->root && !r->live_resources;
    enif_mutex_unlock(r->mutex);
    if (destroy) {
        enif_mutex_destroy(r->mutex);
        enif_free(r);
    }
}
