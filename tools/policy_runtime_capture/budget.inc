/* Native-only requested-byte ledger; public Capture channels are separate. */
struct budget_node {
    struct budget_node *left, *right, *parent;
    ErlNifPid key;
    uint32_t flags;
};
struct budget_session {
    struct budget_node node;
    ErlNifMonitor keeper_monitor;
    ErlNifPid owner;
    ErlNifMonitor owner_monitor;
    uint64_t cap_words, cap_pins, spent_words, spent_pins;
    uint64_t reserved_words, reserved_pins, epoch;
    uint32_t phase, flags;
    ErlNifEnv *request_env;
    ERL_NIF_TERM saved_ref;
};
struct budget_registry {
    struct budget_node *root;
    ErlNifMutex *mutex;
    ErlNifResourceType *type;
    uint64_t live_resources;
    uint32_t flags;
    ERL_NIF_TERM atoms[24];
};
_Static_assert(sizeof(struct budget_node) == 40, "budget node LP64 layout");
_Static_assert(sizeof(struct budget_session) == 192, "budget session LP64 layout");
_Static_assert(sizeof(struct budget_registry) == 232, "budget registry LP64 layout");
_Static_assert(sizeof(ErlNifUInt64) == 8, "profile scalar LP64 layout");
_Static_assert(sizeof(struct budget_session) % _Alignof(ErlNifUInt64) == 0 &&
    _Alignof(struct budget_session) >= _Alignof(ErlNifUInt64), "profile tail alignment");
#define ERTS_RBT_PREFIX budget
#define ERTS_RBT_T struct budget_node
#define ERTS_RBT_KEY_T ErlNifPid
#define ERTS_RBT_FLAGS_T uint32_t
#define ERTS_RBT_INIT_EMPTY_TNODE(T) do { \
    (T)->left = NULL; \
    (T)->right = NULL; \
    (T)->parent = NULL; \
    (T)->flags = 0; \
} while (0)
#define ERTS_RBT_IS_RED(T) ((T)->flags & 1U)
#define ERTS_RBT_SET_RED(T) ((T)->flags |= 1U)
#define ERTS_RBT_IS_BLACK(T) (!ERTS_RBT_IS_RED(T))
#define ERTS_RBT_SET_BLACK(T) ((T)->flags &= ~1U)
#define ERTS_RBT_GET_FLAGS(T) ((T)->flags)
#define ERTS_RBT_SET_FLAGS(T, F) ((T)->flags = (F))
#define ERTS_RBT_GET_PARENT(T) ((T)->parent)
#define ERTS_RBT_SET_PARENT(T, P) ((T)->parent = (P))
#define ERTS_RBT_GET_RIGHT(T) ((T)->right)
#define ERTS_RBT_SET_RIGHT(T, R) ((T)->right = (R))
#define ERTS_RBT_GET_LEFT(T) ((T)->left)
#define ERTS_RBT_SET_LEFT(T, L) ((T)->left = (L))
#define ERTS_RBT_GET_KEY(T) ((T)->key)
#define ERTS_RBT_CMP_KEYS(X, Y) enif_compare_pids(&(X), &(Y))
#define ERTS_RBT_WANT_LOOKUP
#define ERTS_RBT_WANT_LOOKUP_INSERT
#define ERTS_RBT_WANT_DELETE
#define ERTS_RBT_UNDEF
#include "erl_rbtree.h"

#define B_IDLE 0U
#define B_CONTROL 1U
#define B_OFFER 2U
#define B_ISSUED 3U
#define B_KEEPER 4U
#define B_CANCELLED 5U
#define B_CLOSED 6U
#define B_BOUND 1U
#define B_READ 2U
#define B_TERMINAL 4U
#define B_WORD_LIMIT 8U
#define B_CLEARING 16U
#define B_OWNER_MON 32U
#define B_KEEPER_MON 64U
#define B_LINKED 128U
#define B_NOTIFIED 256U
#define B_WRAPPER 512U
#define B_PROFILE_BYTES (4 * sizeof(ErlNifUInt64))
#define B_SESSION_BYTES(profiled) (sizeof(struct budget_session) + ((profiled) ? B_PROFILE_BYTES : 0))
#define B_PROFILE_DATA(s) ((ErlNifUInt64 *)((unsigned char *)(s) + sizeof(struct budget_session)))
#define B_ENTRY_EXTRA_W(s) (((s)->flags & B_WRAPPER) ? B_PROFILE_DATA(s)[0] : 0)
#define B_ENTRY_EXTRA_P(s) (((s)->flags & B_WRAPPER) ? B_PROFILE_DATA(s)[1] : 0)
#define B_UP(N, A) (((N) + (A) - 1) / (A) * (A))
#define B_MAX(X, Y) ((X) > (Y) ? (X) : (Y))
#define B_HF(N) ((uint64_t)ERTS_HEAP_FRAG_SIZE(N))
#define B_MRES (sizeof(ErtsMonitorDataExtended) - sizeof(Eterm) + \
    ERTS_REF_THING_SIZE * sizeof(Eterm))
#define B_ENV B_UP(B_UP(sizeof(ErlNifEnv), _Alignof(Process)) + sizeof(Process), \
    B_MAX(_Alignof(ErlNifEnv), _Alignof(Process)))
#define B_RESOURCE_FOR(data_bytes) ERTS_MAGIC_BIN_UNALIGNED_SIZE(offsetof(ErtsResource, data) + \
    B_UP((data_bytes), ERTS_ALLOC_ALIGN_BYTES) + sizeof(ErtsResourceMonitors))
#define B_RESOURCE B_RESOURCE_FOR(sizeof(struct budget_session))
#define B_RECEIVER B_MAX(sizeof(ErtsMessage) + 5 * sizeof(Eterm), \
    sizeof(ErtsMessageRef) + B_HF(6))
#define B_ENTER_P (B_MRES + 56 + B_HF(204))
#define B_READ_P (40 + B_HF(205))
#define B_NORMAL_P (B_ENTER_P + B_HF(203) + B_READ_P)
#define B_NOTIFY_P (96 + B_HF(203) + B_RECEIVER)
#define B_TERMINAL_P (B_NORMAL_P + B_NOTIFY_P)
#define B_FLOOR_W(s) (27 + (((s)->flags & B_WRAPPER) ? B_PROFILE_DATA(s)[2] : 0))
#define B_FLOOR_P(s) (B_TERMINAL_P + (((s)->flags & B_WRAPPER) ? B_PROFILE_DATA(s)[3] : 0))
#define B_REGISTER_P_FOR(data_bytes) (B_RESOURCE_FOR(data_bytes) + B_ENV + B_MRES + 64 + B_HF(205))
#define B_REGISTER_P B_REGISTER_P_FOR(sizeof(struct budget_session))
_Static_assert(B_SESSION_BYTES(1) == 224, "profiled session LP64 layout");
/* Exact hidden mutex/opened-type field composition; no copied private types. */
#ifdef ERTS_ENABLE_LOCK_COUNT
#define B_MUTEX_NAME B_UP(sizeof(ethr_mutex), _Alignof(erts_lcnt_ref_t)) + sizeof(erts_lcnt_ref_t)
#define B_MUTEX_ALIGN0 B_MAX(_Alignof(ethr_mutex), _Alignof(erts_lcnt_ref_t))
#else
#define B_MUTEX_NAME sizeof(ethr_mutex)
#define B_MUTEX_ALIGN0 _Alignof(ethr_mutex)
#endif
#define B_MUTEX_END (B_UP(B_MUTEX_NAME, _Alignof(char *)) + sizeof(char *))
#ifdef ERTS_DYN_LOCK_CHECK
#define B_MUTEX B_UP(B_UP(B_MUTEX_END, _Alignof(erts_dlc_t)) + sizeof(erts_dlc_t), \
    B_MAX(B_MAX(B_MUTEX_ALIGN0, _Alignof(char *)), _Alignof(erts_dlc_t)))
#else
#define B_MUTEX B_UP(B_MUTEX_END, B_MAX(B_MUTEX_ALIGN0, _Alignof(char *)))
#endif
#define B_OPEN_OP (B_UP(sizeof(void *), _Alignof(ErlNifResourceFlags)) + \
    sizeof(ErlNifResourceFlags))
#define B_OPEN_TYPE (B_UP(B_OPEN_OP, _Alignof(ErlNifResourceType *)) + sizeof(ErlNifResourceType *))
#define B_OPENED B_UP(B_UP(B_OPEN_TYPE, _Alignof(ErlNifResourceTypeInit)) + \
    sizeof(ErlNifResourceTypeInit), B_MAX(_Alignof(void *), \
    B_MAX(_Alignof(ErlNifResourceFlags), _Alignof(ErlNifResourceTypeInit))))
#define B_MODULE (sizeof(struct budget_registry) + B_MUTEX + sizeof(ErlNifResourceType) + B_OPENED)
_Static_assert(B_MODULE >= sizeof(struct budget_registry), "nonzero native bootstrap bank");
_Static_assert(B_REGISTER_P + B_TERMINAL_P < UINT32_MAX, "native cohort fits u32 caps");
_Static_assert(B_REGISTER_P_FOR(B_SESSION_BYTES(1)) + B_TERMINAL_P < UINT32_MAX,
    "profiled native cohort fits u32 caps");

/* Validate BOTH prospective totals before any caller mutates R or U. */
static unsigned budget_debit(struct budget_session *s, uint64_t words, uint64_t pins) {
    if (words > s->cap_words) return 1;
    if (pins > s->cap_pins) return 7;
    return 0;
}

static ERL_NIF_TERM budget_get(ErlNifEnv *env, ERL_NIF_TERM handle,
                              const ErlNifUInt64 *epoch, struct budget_session **out) {
    struct budget_registry *r = enif_priv_data(env);
    void *data;
    if (!enif_get_resource(env, handle, r->type, &data)) return r->atoms[6];
    struct budget_session *s = data;
    enif_mutex_lock(r->mutex);
    unsigned reason = s->phase == B_CLOSED ? 4 : 0;
    if (!reason && epoch && *epoch != s->epoch) reason = 6;
    if (reason) {
        enif_mutex_unlock(r->mutex);
        return r->atoms[reason];
    }
    *out = s;
    return 0; /* Success transfers the held registry lock to the caller. */
}

static void budget_notify(ErlNifEnv *env, struct budget_session *s) {
    struct budget_registry *r = enif_priv_data(env);
    enif_mutex_lock(r->mutex);
    int send = s->phase == B_CLOSED && !(s->flags & B_NOTIFIED);
    s->flags |= B_NOTIFIED;
    enif_mutex_unlock(r->mutex);
    if (send) {
        ERL_NIF_TERM message = enif_make_tuple2(env, r->atoms[23], enif_make_resource(env, s));
        (void)enif_send(env, &s->node.key, NULL, message);
    }
}
/* Called locked. One pinned owner clears outside the lock; CLOSED is sticky. */
static void budget_terminate(ErlNifEnv *env, struct budget_session *s) {
    struct budget_registry *r = enif_priv_data(env);
    if (s->flags & B_CLEARING) {
        enif_mutex_unlock(r->mutex);
        return;
    }
    if (s->phase == B_CLOSED && !(s->flags & (B_BOUND | B_OWNER_MON))) {
        enif_keep_resource(s);
        enif_mutex_unlock(r->mutex);
        budget_notify(env, s);
        enif_release_resource(s);
        return;
    }
    uint64_t epoch = s->epoch;
    ErlNifMonitor monitor = s->owner_monitor;
    int monitored = !!(s->flags & B_OWNER_MON);
    /* The current monitor stays authoritative until cleanup finishes. */
    s->flags = (s->flags | B_CLEARING) & ~B_BOUND;
    s->saved_ref = 0;
    enif_keep_resource(s);
    enif_mutex_unlock(r->mutex);
    enif_clear_env(s->request_env);
    if (monitored) (void)enif_demonitor_process(env, s, &monitor);
    enif_mutex_lock(r->mutex);
    if (s->epoch == epoch) {
        s->flags &= ~(B_CLEARING | B_OWNER_MON);
        if (s->phase != B_CLOSED) {
            s->phase = B_IDLE;
            s->flags &= ~(B_BOUND | B_READ | B_TERMINAL | B_WORD_LIMIT);
            s->reserved_words = B_FLOOR_W(s);
            s->reserved_pins = B_FLOOR_P(s);
        }
    }
    int closed = s->phase == B_CLOSED;
    enif_mutex_unlock(r->mutex);
    if (closed) budget_notify(env, s);
    enif_release_resource(s);
}

#include "budget/lifetime.inc"

static ERL_NIF_TERM budget_reserve(ErlNifEnv *env, struct budget_session *s, ERL_NIF_TERM payload) {
    struct budget_registry *r = enif_priv_data(env);
    const ERL_NIF_TERM *items;
    int arity;
    ErlNifUInt64 w, p, uw, up;
    unsigned reason = 6, phase = B_CONTROL;
    if (enif_get_tuple(env, payload, &arity, &items) && arity == 5 &&
        enif_get_uint64(env, items[0], &w) && enif_get_uint64(env, items[1], &p) &&
        enif_get_uint64(env, items[2], &uw) && enif_get_uint64(env, items[3], &up)) {
        if (enif_is_identical(items[4], r->atoms[16])) phase = B_OFFER;
        else if (enif_is_identical(items[4], r->atoms[17])) phase = B_KEEPER;
        else if (!enif_is_identical(items[4], r->atoms[15])) phase = B_CLOSED;
        if (phase == B_CLOSED) reason = 6;
        else if (s->phase == B_CANCELLED) reason = 21;
        else if (s->flags & B_TERMINAL) reason = s->flags & B_WORD_LIMIT ? 1 : 7;
        else if ((s->phase == B_CONTROL || s->phase == B_KEEPER) &&
                 (phase == B_CONTROL || (s->flags & B_BOUND)) &&
                 w >= s->spent_words && p >= s->spent_pins) {
            uint64_t fw = B_FLOOR_W(s) + ((s->flags & B_BOUND) ? 0 : 3) + ((s->flags & B_READ) ? 0 : 5);
            uint64_t fp = B_FLOOR_P(s) + ((s->flags & B_BOUND) ? 0 : B_HF(203)) +
                ((s->flags & B_READ) ? 0 : B_READ_P);
            reason = w > s->cap_words || uw > s->cap_words - w || uw < fw ? 1 :
                (p > s->cap_pins || up > s->cap_pins - p || up < fp ? 7 : 0);
            if (!reason) {
                s->spent_words = w;
                s->spent_pins = p;
                s->reserved_words = uw;
                s->reserved_pins = up;
                s->phase = phase;
            }
        }
    }
    enif_mutex_unlock(r->mutex);
    return r->atoms[reason];
}

static ERL_NIF_TERM budget_settle(ErlNifEnv *env, struct budget_session *s, ERL_NIF_TERM payload) {
    struct budget_registry *r = enif_priv_data(env);
    const ERL_NIF_TERM *items;
    int arity;
    ErlNifUInt64 w, p;
    unsigned reason = 6;
    if (enif_get_tuple(env, payload, &arity, &items) && arity == 3 &&
        enif_get_uint64(env, items[0], &w) && enif_get_uint64(env, items[1], &p)) {
        int close = enif_is_identical(items[2], r->atoms[14]);
        int ready = enif_is_identical(items[2], r->atoms[18]);
        int done = enif_is_identical(items[2], r->atoms[19]) ||
            enif_is_identical(items[2], r->atoms[20]);
        uint64_t fw = B_FLOOR_W(s) + ((s->flags & B_BOUND) ? 0 : 3) + ((s->flags & B_READ) ? 0 : 5);
        uint64_t fp = B_FLOOR_P(s) + ((s->flags & B_BOUND) ? 0 : B_HF(203)) +
            ((s->flags & B_READ) ? 0 : B_READ_P);
        if (close) {
            fw = 0;
            fp = 0;
        }
        if ((close || ready || done) && (close || !(s->flags & B_TERMINAL)) &&
            (!ready || (s->flags & B_BOUND)) &&
            w >= s->spent_words && p >= s->spent_pins &&
            fw <= s->reserved_words && fp <= s->reserved_pins &&
            w - s->spent_words <= s->reserved_words - fw &&
            p - s->spent_pins <= s->reserved_pins - fp &&
            !(ready && s->phase == B_CANCELLED)) {
            s->reserved_words -= w - s->spent_words;
            s->reserved_pins -= p - s->spent_pins;
            s->spent_words = w;
            s->spent_pins = p;
            if (ready) s->phase = B_KEEPER;
            if (close) {
                s->spent_words += s->reserved_words;
                s->spent_pins += s->reserved_pins;
                s->reserved_words = 0;
                s->reserved_pins = 0;
                s->phase = B_CLOSED;
            }
            if (!ready) {
                budget_terminate(env, s);
                return r->atoms[0];
            }
            reason = 0;
        }
    }
    enif_mutex_unlock(r->mutex);
    return r->atoms[reason];
}

static ERL_NIF_TERM budget_register(ErlNifEnv *env, int argc, const ERL_NIF_TERM argv[]) {
    struct budget_registry *r = enif_priv_data(env);
    ErlNifUInt64 lw, lp, w, p, extras[4] = {0, 0, 0, 0};
    ErlNifPid keeper;
    if (argc != 4 || !enif_self(env, &keeper) ||
        !enif_get_uint64(env, argv[0], &lw) || !enif_get_uint64(env, argv[1], &lp) ||
        lw > UINT32_MAX || lp > UINT32_MAX) return r->atoms[6];
    int profiled = !(enif_get_uint64(env, argv[2], &w) && enif_get_uint64(env, argv[3], &p));
    if (profiled) {
        const ERL_NIF_TERM *words, *pins;
        int word_arity, pin_arity;
        if (!enif_get_tuple(env, argv[2], &word_arity, &words) || word_arity != 3 ||
            !enif_get_tuple(env, argv[3], &pin_arity, &pins) || pin_arity != 3 ||
            !enif_get_uint64(env, words[0], &w) || !enif_get_uint64(env, pins[0], &p) ||
            !enif_get_uint64(env, words[1], &extras[0]) ||
            !enif_get_uint64(env, pins[1], &extras[1]) ||
            !enif_get_uint64(env, words[2], &extras[2]) ||
            !enif_get_uint64(env, pins[2], &extras[3])) return r->atoms[6];
        for (unsigned i = 0; i < 4; i++) {
            if (extras[i] > UINT32_MAX) return r->atoms[6];
        }
    }
    size_t bytes = B_SESSION_BYTES(profiled);
    uint64_t register_p = B_REGISTER_P_FOR(bytes);
    uint64_t floor_w = 27 + extras[2], floor_p = B_TERMINAL_P + extras[3];
    enif_mutex_lock(r->mutex);
    struct budget_node *old = budget_rbt_lookup(r->root, keeper);
    if (old) {
        unsigned reason = ((struct budget_session *)old)->phase == B_CLOSED ? 4 : 2;
        enif_mutex_unlock(r->mutex);
        return r->atoms[reason];
    }
    unsigned reason = w > lw || 8 > lw - w || floor_w > lw - w - 8 ? 1 :
        (p > lp || register_p > lp - p || floor_p > lp - p - register_p ? 7 : 0);
    if (reason || r->flags) {
        enif_mutex_unlock(r->mutex);
        return r->atoms[reason ? reason : 4];
    }
    struct budget_session *s = enif_alloc_resource(r->type, bytes);
    if (!s) {
        enif_mutex_unlock(r->mutex);
        return r->atoms[4];
    }
    memset(s, 0, bytes);
    r->live_resources++;
    s->flags = profiled ? B_WRAPPER : 0;
    if (s->flags & B_WRAPPER) {
        ErlNifUInt64 *profile = B_PROFILE_DATA(s);
        for (unsigned i = 0; i < 4; i++) {
            profile[i] = extras[i];
        }
    }
    s->node.key = keeper;
    s->cap_words = lw;
    s->cap_pins = lp;
    s->spent_words = w + 8;
    s->spent_pins = p + register_p;
    s->reserved_words = B_FLOOR_W(s);
    s->reserved_pins = B_FLOOR_P(s);
    s->request_env = enif_alloc_env();
    if (!s->request_env || enif_monitor_process(env, s, &keeper, &s->keeper_monitor)) {
        enif_mutex_unlock(r->mutex);
        enif_release_resource(s);
        return r->atoms[4];
    }
    s->flags |= B_KEEPER_MON | B_LINKED;
    struct budget_node *duplicate = budget_rbt_lookup_insert(&r->root, &s->node);
    ASSERT(!duplicate);
    (void)duplicate;
    ERL_NIF_TERM result = enif_make_tuple4(env, r->atoms[0], enif_make_resource(env, s),
        enif_make_uint64(env, s->spent_words), enif_make_uint64(env, s->spent_pins));
    enif_mutex_unlock(r->mutex);
    return result;
}

static ERL_NIF_TERM budget_enter(ErlNifEnv *env, int argc, const ERL_NIF_TERM argv[]) {
    struct budget_registry *r = enif_priv_data(env);
    ErlNifPid keeper, owner;
    if (argc != 1 || !enif_get_local_pid(env, argv[0], &keeper) ||
        !enif_self(env, &owner)) return r->atoms[6];
    if (enif_mutex_trylock(r->mutex)) return r->atoms[3];
    struct budget_node *node = budget_rbt_lookup(r->root, keeper);
    if (!node) {
        unsigned reason = enif_is_process_alive(env, &keeper) ? 5 : 4;
        enif_mutex_unlock(r->mutex);
        return r->atoms[reason];
    }
    struct budget_session *s = (struct budget_session *)node;
    unsigned reason = s->phase == B_CLOSED ? 4 : (s->phase != B_IDLE ? 2 : 0);
    if (s->flags & B_CLEARING) reason = s->phase == B_CLOSED ? 4 : 2;
    if (reason) {
        enif_mutex_unlock(r->mutex);
        return r->atoms[reason];
    }
    uint64_t entry_w = 15 + B_ENTRY_EXTRA_W(s);
    uint64_t entry_p = B_NORMAL_P + B_ENTRY_EXTRA_P(s);
    reason = budget_debit(s, s->spent_words + s->reserved_words + entry_w,
        s->spent_pins + s->reserved_pins + entry_p);
    if (reason) s->flags |= B_TERMINAL | (reason == 1 ? B_WORD_LIMIT : 0);
    else {
        s->reserved_words += entry_w;
        s->reserved_pins += entry_p;
    }
    s->owner = owner;
    s->epoch++; /* Each prior normal entry consumed >=7W under a u32 cap. */
    s->phase = B_CONTROL;
    s->reserved_words -= 7;
    s->reserved_pins -= B_ENTER_P;
    s->spent_words += 7;
    s->spent_pins += B_ENTER_P;
    if (enif_monitor_process(env, s, &owner, &s->owner_monitor)) {
        s->spent_words += s->reserved_words;
        s->spent_pins += s->reserved_pins;
        s->reserved_words = 0;
        s->reserved_pins = 0;
        s->phase = B_CLOSED;
        budget_terminate(env, s);
        return r->atoms[4];
    }
    s->flags |= B_OWNER_MON;
    ERL_NIF_TERM result = enif_make_tuple3(env, r->atoms[s->flags & B_TERMINAL ? 8 : 0],
        enif_make_resource(env, s), enif_make_uint64(env, s->epoch));
    enif_mutex_unlock(r->mutex);
    return result;
}

static ERL_NIF_TERM budget_bind(ErlNifEnv *env, int argc, const ERL_NIF_TERM argv[]) {
    struct budget_registry *r = enif_priv_data(env);
    ErlNifUInt64 epoch;
    ErlNifPid owner;
    if (argc != 3 || !enif_get_uint64(env, argv[1], &epoch) ||
        !enif_is_ref(env, argv[2]) || !enif_self(env, &owner)) return r->atoms[6];
    struct budget_session *s;
    ERL_NIF_TERM result = budget_get(env, argv[0], &epoch, &s);
    if (result) return result;
    if (s->phase != B_CONTROL || (s->flags & (B_BOUND | B_CLEARING)) ||
        enif_compare_pids(&owner, &s->owner)) result = r->atoms[6];
    else {
        s->reserved_words -= 3;
        s->reserved_pins -= B_HF(203);
        s->spent_words += 3;
        s->spent_pins += B_HF(203);
        /* Trusted callers supply actual ordinary make_ref; is_ref alone is not its size proof. */
        s->saved_ref = enif_make_copy(s->request_env, argv[2]);
        s->flags |= B_BOUND;
        result = r->atoms[0];
    }
    enif_mutex_unlock(r->mutex);
    return result;
}

static ERL_NIF_TERM budget_read(ErlNifEnv *env, int argc, const ERL_NIF_TERM argv[]) {
    struct budget_registry *r = enif_priv_data(env);
    ErlNifPid keeper, owner;
    if (argc != 3 || !enif_self(env, &keeper) ||
        !enif_get_local_pid(env, argv[1], &owner) || !enif_is_ref(env, argv[2])) return r->atoms[6];
    struct budget_session *s;
    ERL_NIF_TERM result = budget_get(env, argv[0], NULL, &s);
    if (result) return result;
    unsigned reason = 6;
    if (s->phase != B_IDLE && (s->flags & B_BOUND) && !(s->flags & B_CLEARING) &&
        !enif_compare_pids(&keeper, &s->node.key) && !enif_compare_pids(&owner, &s->owner) &&
        enif_is_identical(s->saved_ref, argv[2]) &&
        !((s->flags & B_TERMINAL) && (s->flags & B_READ))) {
        reason = 0;
        if (s->flags & B_READ) reason = budget_debit(s,
            s->spent_words + s->reserved_words + 5, s->spent_pins + s->reserved_pins + B_READ_P);
        if (!reason) {
            if (!(s->flags & B_READ)) {
                s->reserved_words -= 5;
                s->reserved_pins -= B_READ_P;
            }
            s->spent_words += 5;
            s->spent_pins += B_READ_P;
            s->flags |= B_READ;
            result = enif_make_tuple4(env, r->atoms[0], enif_make_uint64(env, s->epoch),
                enif_make_uint64(env, s->spent_words), enif_make_uint64(env, s->spent_pins));
        }
    }
    enif_mutex_unlock(r->mutex);
    return reason ? r->atoms[reason] : result;
}

static ERL_NIF_TERM budget_control(ErlNifEnv *env, int argc, const ERL_NIF_TERM argv[]) {
    struct budget_registry *r = enif_priv_data(env);
    ErlNifUInt64 epoch;
    ErlNifPid self;
    if (argc != 4 || !enif_get_uint64(env, argv[1], &epoch) ||
        !enif_self(env, &self)) return r->atoms[6];
    struct budget_session *s;
    ERL_NIF_TERM result = budget_get(env, argv[0], &epoch, &s);
    if (result) return result;
    int keeper = !enif_compare_pids(&self, &s->node.key);
    int owner = s->phase != B_IDLE && !enif_compare_pids(&self, &s->owner);
    if (!(s->flags & B_CLEARING) && keeper && enif_is_identical(argv[2], r->atoms[9]))
        return budget_reserve(env, s, argv[3]);
    if (!(s->flags & B_CLEARING) && keeper && enif_is_identical(argv[2], r->atoms[12]))
        return budget_settle(env, s, argv[3]);
    char nil[4];
    int no_payload = enif_get_atom(env, argv[3], nil, sizeof(nil), ERL_NIF_LATIN1) == 4 &&
        !strcmp(nil, "nil");
    int close = no_payload && ((keeper && enif_is_identical(argv[2], r->atoms[14])) ||
        (owner && enif_is_identical(argv[2], r->atoms[13])));
    if (close) {
        s->spent_words += s->reserved_words;
        s->spent_pins += s->reserved_pins;
        s->reserved_words = 0;
        s->reserved_pins = 0;
        s->phase = B_CLOSED;
        budget_terminate(env, s);
        return r->atoms[0];
    }
    result = r->atoms[6];
    if (no_payload && owner && !(s->flags & B_CLEARING)) {
        if (s->phase == B_OFFER && enif_is_identical(argv[2], r->atoms[10])) {
            s->phase = B_ISSUED;
            result = r->atoms[0];
        } else if ((s->phase == B_CONTROL || s->phase == B_OFFER || s->phase == B_CANCELLED) &&
                   enif_is_identical(argv[2], r->atoms[11])) {
            s->phase = B_CANCELLED;
            result = r->atoms[0];
        }
    }
    enif_mutex_unlock(r->mutex);
    return result;
}
