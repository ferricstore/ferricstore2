/* Exact OTP-28.3.1 source instrument; never linked into the Rust runtime. */
#include "sys.h"
#include "global.h"
#include "erl_nif.h"
#include "erl_fun.h"
#include "erl_node_tables.h"
#include "erl_bits.h"
#include "erl_map.h"
#include "erl_utils.h"
#include "external.h"
#include "erl_zlib.h"
#include "export.h"
#include <stdint.h>
#include <string.h>

#ifndef FS_CAPTURE_HEADER_SHA256
#error "Define FS_CAPTURE_HEADER_SHA256 from the actual compiler header manifest"
#endif
#define SOURCE_REVISION "aa5b1d3366762ff8f420f2aa6a52604b481530c9"
#if defined(DEBUG) || defined(DBG_HASHMAP_COLLISION_BONANZA) || defined(WORDS_BIGENDIAN)
#error "The capture ABI requires the selected optimized little-endian release"
#endif
_Static_assert(sizeof(Eterm) == 8 && sizeof(ERL_NIF_TERM) == 8, "capture requires 64-bit terms");

#define DONOR 0
#define WORDS 1
#define TERMS 2
#define FACTS 3
#define TEXT 4
#define DEPTH 5
#define PINS 6
#define OUTPUT 7
static const char *cap_names[8] = {"donor_bytes", "decoded_words", "terms", "facts",
    "text_bytes", "depth", "pinned_bytes", "output_bytes"};
static const char *result_names[5] = {"hashes", "atoms", "nodes", "exports", "local_funs"};

/* Approved call-local state: no borrowed environment or term escapes the call. */
struct capture_state {
    ErlNifEnv *env;
    uint64_t cap[8];
    uint64_t used[8];
    ERL_NIF_TERM lists[5];
    int emitting;
    const char *refusal;
};

static ERL_NIF_TERM atom(ErlNifEnv *env, const char *name) { return enif_make_atom(env, name); }
static ERL_NIF_TERM error(ErlNifEnv *env, const char *reason) {
    return enif_make_tuple2(env, atom(env, "error"), atom(env, reason));
}
static ERL_NIF_TERM binary(ErlNifEnv *env, const void *bytes, size_t length) {
    ERL_NIF_TERM result;
    unsigned char *out = enif_make_new_binary(env, length, &result);
    if (length && !out) return enif_raise_exception(env, atom(env, "allocation_failure"));
    if (length) memcpy(out, bytes, length);
    return result;
}
static ERL_NIF_TERM text(ErlNifEnv *env, const char *value) {
    return binary(env, value, strlen(value));
}
static ERL_NIF_TERM map(ErlNifEnv *env, const char **names, ERL_NIF_TERM *values, unsigned n) {
    ERL_NIF_TERM keys[12], result;
    if (n > 12) return enif_raise_exception(env, atom(env, "invalid_native_map"));
    for (unsigned i = 0; i < n; i++) keys[i] = atom(env, names[i]);
    if (!enif_make_map_from_arrays(env, keys, values, n, &result)) return enif_make_badarg(env);
    return result;
}
static int debit(struct capture_state *s, unsigned field, uint64_t amount) {
    if (UINT64_MAX - s->used[field] < amount || s->used[field] + amount > s->cap[field]) {
        s->refusal = cap_names[field];
        return 0;
    }
    s->used[field] += amount;
    return 1;
}
static int name_bytes(struct capture_state *s, Eterm value, char *bytes, unsigned *length) {
    if (!enif_get_atom_length(s->env, value, length, ERL_NIF_UTF8) || *length > 1020 ||
        !enif_get_atom(s->env, value, bytes, *length + 1, ERL_NIF_UTF8)) {
        s->refusal = "invalid_atom";
        return 0;
    }
    return debit(s, TEXT, *length);
}
/* Small emitted maps: 4+2n words, two list spines (append then reverse),
 * and at most two words for each unsigned64. Binary payloads are separate. */
static int fact_room(struct capture_state *s, unsigned n, uint64_t payload,
                     uint64_t integers, const char **keys) {
    uint64_t encoded = 5 + payload + 5 * (n - integers) + 11 * integers;
    uint64_t words = 4 + 2 * n + 4 + 2 * integers;
    for (unsigned i = 0; i < n; i++) encoded += 3 + strlen(keys[i]);
    /* Each binary has either its heap header or BinRef+SubBits; counting
     * both and every payload is conservative across the64-byte boundary. */
    words += (n - integers) * (2 + ERL_REFC_BITS_SIZE) + (payload + 7) / 8;
    return debit(s, FACTS, 1) && debit(s, OUTPUT, encoded) &&
        debit(s, PINS, words * 8 + (n - integers) * sizeof(Binary));
}
static void push_fact(struct capture_state *s, unsigned kind, ERL_NIF_TERM item) {
    s->lists[kind] = enif_make_list_cell(s->env, item, s->lists[kind]);
}
static int walk(struct capture_state *, Eterm, uint64_t);

static int atom_fact(struct capture_state *s, Eterm term) {
    if (s->emitting == 2) return 1;
    char name[1021]; unsigned length;
    const char *keys[] = {"name", "native_word"};
    if (!name_bytes(s, term, name, &length) || !fact_room(s, 2, length, 1, keys)) return 0;
    if (s->emitting == 1) {
        ERL_NIF_TERM values[] = {binary(s->env, name, length), enif_make_uint64(s->env, (uint64_t)term)};
        push_fact(s, 1, map(s->env, keys, values, 2));
    }
    return 1;
}
static int node_fact(struct capture_state *s, Eterm term, uint64_t depth) {
    const ExternalThing *thing = external_thing_ptr(term);
    if (s->emitting == 2) return walk(s, thing->node->sysname, depth + 1);
    char name[1021]; unsigned length;
    const char *keys[] = {"name", "creation", "hash_input"};
    if (!name_bytes(s, thing->node->sysname, name, &length) || !fact_room(s, 3, length, 2, keys)) return 0;
    if (s->emitting == 1) {
        ERL_NIF_TERM values[] = {binary(s->env, name, length),
            enif_make_uint(s->env, thing->node->creation),
            enif_make_uint64(s->env, (uint64_t)(uintptr_t)thing->node)};
        push_fact(s, 2, map(s->env, keys, values, 3));
    }
    return walk(s, thing->node->sysname, depth + 1);
}
static int export_fact(struct capture_state *s, const ErlFunThing *fun, uint64_t depth) {
    const ErtsCodeMFA *mfa = &fun->entry.exp->info.mfa;
    if (s->emitting == 2)
        return walk(s, mfa->module, depth + 1) && walk(s, mfa->function, depth + 1);
    char module[1021], function[1021]; unsigned ml, fl;
    const char *keys[] = {"module", "function", "arity", "hash_input"};
    if (!name_bytes(s, mfa->module, module, &ml) || !name_bytes(s, mfa->function, function, &fl) ||
        !fact_room(s, 4, ml + fl, 2, keys)) return 0;
    if (s->emitting == 1) {
        ERL_NIF_TERM values[] = {binary(s->env, module, ml), binary(s->env, function, fl),
            enif_make_uint(s->env, mfa->arity),
            enif_make_uint64(s->env, (uint64_t)(uintptr_t)fun->entry.exp)};
        push_fact(s, 3, map(s->env, keys, values, 4));
    }
    return walk(s, mfa->module, depth + 1) && walk(s, mfa->function, depth + 1);
}
static int local_fact(struct capture_state *s, const ErlFunThing *fun, uint64_t depth) {
    const ErlFunEntry *entry = fun->entry.fun;
    if (s->emitting == 2) {
        if (!walk(s, entry->module, depth + 1)) return 0;
        for (unsigned i = 0; i < fun_num_free(fun); i++)
            if (!walk(s, fun->env[i], depth + 1)) return 0;
        return 1;
    }
    char module[1021]; unsigned length;
    const char *keys[] = {"module", "index", "arity", "uniq", "old_uniq"};
    if (!name_bytes(s, entry->module, module, &length) || !fact_room(s, 5, length + 16, 3, keys)) return 0;
    if (s->emitting == 1) {
        ERL_NIF_TERM values[] = {binary(s->env, module, length), enif_make_int(s->env, entry->index),
            enif_make_uint(s->env, fun_arity(fun)), binary(s->env, entry->uniq, 16),
            enif_make_int(s->env, entry->old_uniq)};
        push_fact(s, 4, map(s->env, keys, values, 5));
    }
    if (!walk(s, entry->module, depth + 1)) return 0;
    for (unsigned i = 0; i < fun_num_free(fun); i++) if (!walk(s, fun->env[i], depth + 1)) return 0;
    return 1;
}
static int walk_fun(struct capture_state *s, Eterm term, uint64_t depth) {
    const ErlFunThing *fun = (const ErlFunThing *)fun_val(term);
    if (!debit(s, WORDS, ERL_FUN_SIZE + fun_num_free(fun))) return 0;
    return is_local_fun(fun) ? local_fact(s, fun, depth) : export_fact(s, fun, depth);
}
static int walk_hamt(struct capture_state *s, Eterm term, uint64_t depth, unsigned level) {
    const Eterm *p = boxed_val(term);
    Eterm header = *p++;
    unsigned count, head = !is_arity_value(header) && is_hashmap_header_head(header);
    if (level > HAMT_MAX_LEVEL) { s->refusal = "invalid_term"; return 0; }
    if (is_arity_value(header)) count = header_arity(header); /* full-hash collision tuple */
    else {
        if (head) p++; /* stored map cardinality */
        count = (header & _HEADER_MAP_SUBTAG_MASK) == HAMT_SUBTAG_HEAD_ARRAY ? 16 :
            hashmap_bitcount(MAP_HEADER_VAL(header));
    }
    if (!debit(s, WORDS, 1 + head + count)) return 0;
    for (unsigned i = 0; i < count; i++) {
        if (is_list(p[i])) {
            const Eterm *pair = list_val(p[i]);
            if (!debit(s, WORDS, 2) || !walk(s, CAR(pair), depth + 1) ||
                !walk(s, CDR(pair), depth + 1)) return 0;
        } else if (!walk_hamt(s, p[i], depth, level + 1)) return 0;
    }
    return 1;
}
static int walk_map(struct capture_state *s, Eterm term, uint64_t depth) {
    if (is_hashmap(term)) return walk_hamt(s, term, depth, 0);
    const flatmap_t *mp = (const flatmap_t *)flatmap_val(term);
    Uint n = flatmap_get_size(mp);
    const Eterm *keys = flatmap_get_keys(mp), *values = flatmap_get_values(mp);
    if (!debit(s, WORDS, MAP_HEADER_FLATMAP_SZ + (n ? 1 + n : 0) + n)) return 0;
    for (Uint i = 0; i < n; i++)
        if (!walk(s, keys[i], depth + 1) || !walk(s, values[i], depth + 1)) return 0;
    return 1;
}
static int walk_list(struct capture_state *s, Eterm term, uint64_t depth) {
    ERL_NIF_TERM head, tail = term;
    while (enif_get_list_cell(s->env, tail, &head, &tail)) {
        if (!debit(s, WORDS, 2) || !walk(s, head, depth + 1)) return 0;
    }
    return enif_is_empty_list(s->env, tail) || walk(s, tail, depth + 1);
}
static int walk(struct capture_state *s, Eterm term, uint64_t depth) {
    if (depth > s->cap[DEPTH]) { s->refusal = "depth"; return 0; }
    if (!debit(s, TERMS, 1)) return 0;
    if (enif_is_atom(s->env, term)) return atom_fact(s, term);
    if (enif_is_empty_list(s->env, term)) return 1;
    if (enif_is_list(s->env, term)) return walk_list(s, term, depth);
    const ERL_NIF_TERM *items; int arity;
    if (enif_get_tuple(s->env, term, &arity, &items)) {
        if (!debit(s, WORDS, (uint64_t)arity + 1)) return 0;
        for (int i = 0; i < arity; i++) if (!walk(s, items[i], depth + 1)) return 0;
        return 1;
    }
    if (enif_is_map(s->env, term)) return walk_map(s, term, depth);
    if (enif_is_fun(s->env, term)) return walk_fun(s, term, depth);
    if (is_external(term)) {
        if (!debit(s, WORDS, 1 + header_arity(*boxed_val(term)))) return 0;
        return node_fact(s, term, depth);
    }
    if (is_bitstring(term) && *boxed_val(term) == HEADER_SUB_BITS) {
        const ErlSubBits *sub = (const ErlSubBits *)boxed_val(term);
        const BinRef *ref = (const BinRef *)boxed_val(sub->orig);
        if (ref->thing_word != HEADER_BIN_REF || ref->val->orig_size < 0) {
            s->refusal = "invalid_term"; return 0;
        }
        /* Count the backing, not merely this window. Hashing may also
         * require one byte-alignment copy of the visible bitstring. */
        return debit(s, WORDS, ERL_REFC_BITS_SIZE) && debit(s, PINS,
            sizeof(Binary) + (uint64_t)ref->val->orig_size +
            (s->emitting == 2 ? 0 : (sub->end - sub->start + 7) / 8));
    }
    if (is_internal_magic_ref(term)) {
        const ErtsMRefThing *ref = magic_ref_thing_ptr(term);
        if (ref->mb->orig_size < 0) { s->refusal = "invalid_term"; return 0; }
        /* VM-known allocation only; no theorem about external graphs
         * owned by a generic resource destructor. Never read a wire word. */
        return debit(s, WORDS, ERTS_MAGIC_REF_THING_SIZE) &&
            debit(s, PINS, sizeof(ErtsMagicBinary) + (uint64_t)ref->mb->orig_size);
    }
    if (is_boxed(term)) return debit(s, WORDS, 1 + header_arity(*boxed_val(term)));
    return 1;
}
static int roots(struct capture_state *s, ERL_NIF_TERM list) {
    ERL_NIF_TERM item, tail = list;
    while (enif_get_list_cell(s->env, tail, &item, &tail)) {
        if (!walk(s, item, 0)) return 0;
        if (!debit(s, OUTPUT, 11) || !debit(s, PINS, 6 * sizeof(Eterm))) return 0;
        if (s->emitting == 1) push_fact(s, 0, enif_make_uint64(s->env,
            enif_hash(ERL_NIF_INTERNAL_HASH, item, 0)));
    }
    if (!enif_is_empty_list(s->env, tail)) { s->refusal = "invalid_terms"; return 0; }
    return 1;
}
#include "policy_runtime_capture/raw.inc"
/* Phase2 counts original live owners without facts, names, hashes or decoding.
 * Fixed carrier/query wrappers consume actual heap, not source-policy depth. */
static int live_list(struct capture_state *, Eterm, unsigned);
static int live_donor(struct capture_state *, Eterm);
static int live_map(struct capture_state *, Eterm, unsigned);
static int live_binary(struct capture_state *s, Eterm term) {
    if (!enif_is_binary(s->env, term)) { s->refusal = "invalid_donors"; return 0; }
    const byte *base; Uint offset, bits;
    ERTS_GET_BITSTRING(term, base, offset, bits);
    (void)base; (void)offset;
    return debit(s, DONOR, bits / 8) && walk(s, term, 0);
}
static int live_pair(struct capture_state *s, Eterm key, Eterm value, unsigned kind) {
    /* kind1=query map, kind2=exact anchored donor; no term-key hashing. */
    if (kind == 2) {
        if (enif_is_identical(key, atom(s->env, "encoded")))
            return walk(s, key, 0) && live_binary(s, value);
        if (!enif_is_identical(key, atom(s->env, "source_term"))) {
            s->refusal = "invalid_donors"; return 0;
        }
    }
    if (!walk(s, key, 0)) return 0;
    if (kind == 1 && enif_is_identical(key, atom(s->env, "atoms")))
        return live_list(s, value, 0);
    if (kind == 1 && enif_is_identical(key, atom(s->env, "exports")))
        return live_list(s, value, 1);
    return walk(s, value, 0);
}
static int live_hamt(struct capture_state *s, Eterm term, unsigned kind, unsigned level) {
    const Eterm *p = boxed_val(term);
    Eterm header = *p++;
    unsigned count, head = !is_arity_value(header) && is_hashmap_header_head(header);
    if (level > HAMT_MAX_LEVEL) { s->refusal = "invalid_term"; return 0; }
    if (is_arity_value(header)) count = header_arity(header);
    else {
        if (head) p++;
        count = (header & _HEADER_MAP_SUBTAG_MASK) == HAMT_SUBTAG_HEAD_ARRAY ? 16 :
            hashmap_bitcount(MAP_HEADER_VAL(header));
    }
    if (!debit(s, WORDS, 1 + head + count)) return 0;
    for (unsigned i = 0; i < count; i++) {
        if (is_list(p[i])) {
            const Eterm *pair = list_val(p[i]);
            if (!debit(s, WORDS, 2) || !live_pair(s, CAR(pair), CDR(pair), kind)) return 0;
        } else if (!live_hamt(s, p[i], kind, level + 1)) return 0;
    }
    return 1;
}
static int live_map(struct capture_state *s, Eterm term, unsigned kind) {
    if (is_hashmap(term)) return live_hamt(s, term, kind, 0);
    const flatmap_t *mp = (const flatmap_t *)flatmap_val(term);
    Uint n = flatmap_get_size(mp);
    const Eterm *keys = flatmap_get_keys(mp), *values = flatmap_get_values(mp);
    if (!debit(s, WORDS, MAP_HEADER_FLATMAP_SZ + (n ? 1 + n : 0) + n)) return 0;
    for (Uint i = 0; i < n; i++) if (!live_pair(s, keys[i], values[i], kind)) return 0;
    return 1;
}
static int live_donor(struct capture_state *s, Eterm term) {
    size_t size;
    if (enif_is_binary(s->env, term)) return live_binary(s, term);
    if (!enif_get_map_size(s->env, term, &size) || size != 2) {
        s->refusal = "invalid_donors"; return 0;
    }
    /* Unique map keys + exact size + pair classification prove exact shape. */
    return debit(s, TERMS, 1) && live_map(s, term, 2);
}
static int live_list(struct capture_state *s, Eterm term, unsigned kind) {
    Eterm item, tail = term;
    if (!enif_is_list(s->env, term)) return walk(s, term, 0);
    if (!debit(s, TERMS, 1)) return 0;
    while (enif_get_list_cell(s->env, tail, &item, &tail)) {
        if (!debit(s, WORDS, 2)) return 0;
        if (kind == 2) { if (!live_donor(s, item)) return 0; }
        else if (kind == 1) {
            const ERL_NIF_TERM *fields; int arity;
            if (enif_get_tuple(s->env, item, &arity, &fields) && arity == 3) {
                if (!debit(s, TERMS, 1) || !debit(s, WORDS, 4)) return 0;
                for (int i = 0; i < 3; i++) if (!walk(s, fields[i], 0)) return 0;
            } else if (!walk(s, item, 0)) return 0;
        } else if (!walk(s, item, 0)) return 0;
    }
    if (enif_is_empty_list(s->env, tail)) return 1;
    if (kind == 2) { s->refusal = "invalid_donors"; return 0; }
    return walk(s, tail, 0);
}
static ERL_NIF_TERM live_preflight(struct capture_state *s, Eterm carrier) {
    Eterm donors, queries, tail;
    if (!enif_get_list_cell(s->env, carrier, &donors, &tail) ||
        !enif_get_list_cell(s->env, tail, &queries, &tail) ||
        !enif_is_empty_list(s->env, tail) || !enif_is_list(s->env, donors) ||
        !enif_is_map(s->env, queries)) return error(s->env, "invalid_donors");
    s->emitting = 2;
    /* Two protocol tuples + two-cell carrier; count their five term visits. */
    if (!debit(s, WORDS, 10) || !debit(s, TERMS, 5) || !live_list(s, donors, 2) ||
        !debit(s, TERMS, 1) || !live_map(s, queries, 1)) return refused(s);
    /* enif_make_map_from_arrays(3):10 words, <=6 for three uint64 boxes,
     * plus the3-word ok tuple. Admit BEFORE constructing control output.
     * Returned word/pin envelopes include this fixed report allowance. */
    uint64_t report_words = MAP_HEADER_FLATMAP_SZ + 7 + 6 + 3;
    if (!debit(s, WORDS, report_words) || !debit(s, PINS, s->used[WORDS] * 8)) return refused(s);
    const char *keys[] = {"live_words", "live_pinned_bytes", "live_terms"};
    ERL_NIF_TERM values[] = {enif_make_uint64(s->env, s->used[WORDS]),
        enif_make_uint64(s->env, s->used[PINS]), enif_make_uint64(s->env, s->used[TERMS])};
    return enif_make_tuple2(s->env, atom(s->env, "ok"), map(s->env, keys, values, 3));
}
static ERL_NIF_TERM capture_terms(ErlNifEnv *env, int argc, const ERL_NIF_TERM argv[]) {
    struct capture_state state = {.env = env};
    ERL_NIF_TERM cap;
    if (argc != 2) return enif_make_badarg(env);
    for (unsigned i = 0; i < 8; i++) {
        ErlNifUInt64 number;
        if (!enif_get_map_value(env, argv[1], atom(env, cap_names[i]), &cap) ||
            !enif_get_uint64(env, cap, &number) || number > UINT32_MAX)
            return error(env, "invalid_limits");
        state.cap[i] = (uint64_t)number;
    }
    if (state.cap[DEPTH] > 64) return error(env, "invalid_limits");
    const ERL_NIF_TERM *mode; int arity;
    if (enif_get_tuple(env, argv[0], &arity, &mode) && arity == 2 &&
        enif_is_identical(mode[0], atom(env, "preflight"))) {
        const ERL_NIF_TERM *inner; int count;
        if (enif_get_tuple(env, mode[1], &count, &inner) && count == 2 &&
            enif_is_identical(inner[0], atom(env, "anchors"))) return live_preflight(&state, inner[1]);
        return preflight(&state, mode[1]);
    }
    /* version + {ok, map}, five key atoms and five proper-list headers. */
    uint64_t output = 1 + 2 + 5 + 5 + 5 * 6;
    for (unsigned i = 0; i < 5; i++) output += 3 + strlen(result_names[i]);
    if (!debit(&state, OUTPUT, output) || !debit(&state, PINS, (4 + 2 * 5 + 3) * 8) ||
        !roots(&state, argv[0])) return refused(&state);
    /* Actual input heap, output heap above, and the geometric hash-stack
     * frontier coexist. A read-only map walker allocates no iterator. */
    if (!debit(&state, PINS, (state.used[WORDS] + 6 * state.used[TERMS] +
        (state.used[WORDS] ? 3 * HAMT_MAX_LEVEL * 16 : 0)) * 8)) return refused(&state);
    memset(state.used, 0, sizeof(state.used));
    state.emitting = 1;
    for (unsigned i = 0; i < 5; i++) state.lists[i] = enif_make_list(env, 0);
    if (!roots(&state, argv[0])) return refused(&state);
    ERL_NIF_TERM values[5];
    for (unsigned i = 0; i < 5; i++) if (!enif_make_reverse_list(env, state.lists[i], &values[i]))
        return error(env, "invalid_native_list");
    return enif_make_tuple2(env, atom(env, "ok"), map(env, result_names, values, 5));
}
static int hex_digest(unsigned char *out) {
    const char *hex = FS_CAPTURE_HEADER_SHA256;
    if (strlen(hex) != 64) return 0;
    for (unsigned i = 0; i < 32; i++) {
        unsigned value = 0;
        for (unsigned j = 0; j < 2; j++) {
            char c = hex[2 * i + j];
            if (c >= '0' && c <= '9') value = value * 16 + (unsigned)(c - '0');
            else if (c >= 'a' && c <= 'f') value = value * 16 + (unsigned)(c - 'a' + 10);
            else return 0;
        }
        out[i] = (unsigned char)value;
    }
    return 1;
}
static ERL_NIF_TERM abi(ErlNifEnv *env, int argc, const ERL_NIF_TERM argv[]) {
    (void)argc; (void)argv;
    unsigned char digest[32];
    if (!hex_digest(digest)) return error(env, "invalid_compiled_manifest");
    const char *keys[] = {"otp_package", "otp_release", "erts_version", "build_type", "word_size",
        "endianness", "source_revision", "header_digest"};
    ERL_NIF_TERM values[] = {text(env, "28.3.1"), text(env, "28"), text(env, "16.2"), atom(env, "opt"),
        enif_make_uint(env, 8), atom(env, "little"), text(env, SOURCE_REVISION), binary(env, digest, 32)};
    return map(env, keys, values, 8);
}
static int load(ErlNifEnv *env, void **private_data, ERL_NIF_TERM info) {
    ErlNifSysInfo actual;
    (void)private_data;
    enif_system_info(&actual, sizeof(actual));
    if (strcmp(actual.otp_release, "28") || strcmp(actual.erts_version, "16.2") ||
        actual.nif_major_version != ERL_NIF_MAJOR_VERSION ||
        actual.nif_minor_version < ERL_NIF_MINOR_VERSION) return -1;
    unsigned char endian[8]; uint64_t word = 1;
    memcpy(endian, &word, sizeof(word));
    if (endian[0] != 1 || !enif_is_identical(info, abi(env, 0, NULL))) return -1;
    const char *names[] = {"name", "native_word", "creation", "hash_input", "module", "function",
        "arity", "uniq", "index", "old_uniq", "ok", "error", "limit", "allocation_failure",
        "preflight", "anchors", "encoded", "source_term", "live_words", "live_pinned_bytes", "live_terms"};
    for (unsigned i = 0; i < sizeof(names)/sizeof(names[0]); i++) (void)atom(env, names[i]);
    for (unsigned i = 0; i < 8; i++) (void)atom(env, cap_names[i]);
    for (unsigned i = 0; i < 5; i++) (void)atom(env, result_names[i]);
    return 0;
}
static ErlNifFunc functions[] = {
    {"abi", 0, abi, 0}, {"capture_terms", 2, capture_terms, ERL_NIF_DIRTY_JOB_CPU_BOUND}
};
ERL_NIF_INIT(Elixir.Ferricstore.PolicyRuntimeCapture.Native, functions, load, NULL, NULL, NULL)
