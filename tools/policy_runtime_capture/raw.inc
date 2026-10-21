static int raw_take(struct capture_state *s, const byte **p, const byte *end, uint64_t n) {
    if (n > (uint64_t)(end - *p)) { s->refusal = "invalid_external_term"; return 0; }
    *p += n;
    return 1;
}
static int raw_uint(struct capture_state *s, const byte **p, const byte *end,
                    unsigned bytes, uint64_t *n) {
    const byte *start = *p;
    if (!raw_take(s, p, end, bytes)) return 0;
    *n = 0;
    for (unsigned i = 0; i < bytes; i++) *n = (*n << 8) | start[i];
    return 1;
}
static int raw_term(struct capture_state *, const byte **, const byte *, uint64_t);
static int raw_many(struct capture_state *s, const byte **p, const byte *end,
                    uint64_t n, uint64_t depth) {
    if (n > (uint64_t)(end - *p)) { s->refusal = "invalid_external_term"; return 0; }
    for (uint64_t i = 0; i < n; i++) if (!raw_term(s, p, end, depth)) return 0;
    return 1;
}
static int raw_map(struct capture_state *s, const byte **p, const byte *end,
                   uint64_t n, uint64_t depth) {
    uint64_t keys = 0;
    for (uint64_t i = 0; i < n; i++) {
        const byte *start = *p;
        if (!raw_term(s, p, end, depth + 1)) return 0;
        /* Exact lower bound for encoded binary keys in a ROOT map. This
         * rejects an oversized order BEFORE expensive native decoding. */
        if (!depth && start < end && *start == 109) keys += 1 + (uint64_t)(*p - start);
        if (keys > s->cap[OUTPUT]) { s->refusal = "output_bytes"; return 0; }
        if (!raw_term(s, p, end, depth + 1)) return 0;
    }
    /* Keep OTP's preallocation, the constructed HAMT and every possible
     * extra fragment live together. Source: external.c5233; map.c867ff.
     * <=1+16n requests,200 slack/request;33n+2 includes leaves. */
    uint64_t words = n <= MAP_SMALL_MAP_LIMIT ? 3 + n + (n ? 1 + n : 0) :
        HASHMAP_ESTIMATED_HEAP_SIZE(n) + 33 * n + 2 +
        (1 + 16 * n) * (200 + (sizeof(ErlHeapFragment) + 7) / 8) + 4 * n;
    /* Deferred map descriptor (three words), geometric old/new capacity
     * <=6 slots per item, and a trie stack <=16 levels *17 words. */
    return debit(s, WORDS, words + 18 + (n > 32 ? 3 * 16 * 17 : 0));
}
static int raw_external(struct capture_state *s, const byte **p, const byte *end,
                        uint64_t tag, uint64_t depth) {
    uint64_t n = 1;
    int ref = tag == 101 || tag == 114 || tag == 90;
    if (ref && tag != 101 && !raw_uint(s, p, end, 2, &n)) return 0;
    if (n > ERTS_MAX_REF_NUMBERS || !raw_term(s, p, end, depth + 1)) return 0;
    unsigned creation = tag == 88 || tag == 89 || tag == 90 || tag == 120 ? 4 : 1;
    uint64_t tail = ref ? 4 * n : tag == 120 ? 8 : tag == 88 || tag == 103 ? 8 : 4;
    if ((uint64_t)(end - *p) < creation + tail) { s->refusal = "invalid_external_term"; return 0; }
    /* Only this bound donor's COMPLETE live walk can authorize phase4.
     * Plain binaries still require a source anchor for a possible magic ref. */
    if (s->emitting != 4 && ref && n > 1 && ((*p)[creation + 4 + 1] & 2)) {
        s->refusal = "source_anchor_required";
        return 0;
    }
    uint64_t words = ref ? EXTERNAL_THING_HEAD_SIZE + n / 2 + 1 : EXTERNAL_PID_HEAP_SIZE;
    return raw_take(s, p, end, creation + tail) && debit(s, WORDS, words) &&
        debit(s, FACTS, 1) && debit(s, TEXT, 1020);
}
static int raw_payload(struct capture_state *s, const byte **p, const byte *end,
                       uint64_t tag, uint64_t n) {
    if (tag == 109 || tag == 77) {
        if (!raw_take(s, p, end, n + (tag == 77))) return 0;
        uint64_t words = n <= ERL_ONHEAP_BINARY_LIMIT ? heap_bits_size(n * 8) : ERL_REFC_BITS_SIZE;
        return debit(s, WORDS, words) && (n <= 64 || debit(s, PINS, sizeof(Binary) + n));
    }
    if (tag == 110 || tag == 111)
        return raw_take(s, p, end, n + 1) && debit(s, WORDS, 2 + (n + 7) / 8);
    return raw_take(s, p, end, n) && debit(s, FACTS, 1) &&
        debit(s, TEXT, n * (tag == 100 || tag == 115 ? 2 : 1));
}
static int raw_term(struct capture_state *s, const byte **p, const byte *end, uint64_t depth) {
    uint64_t tag, n;
    for (;;) {
        if (depth > s->cap[DEPTH]) { s->refusal = "depth"; return 0; }
        if (!debit(s, TERMS, 1) || !raw_uint(s, p, end, 1, &tag)) return 0;
        switch (tag) {
        case 97: return raw_take(s, p, end, 1);
        case 98: return raw_take(s, p, end, 4);
        case 99: case 70:
            return raw_take(s, p, end, tag == 99 ? 31 : 8) && debit(s, WORDS, FLOAT_SIZE_OBJECT);
        case 106: return 1;
        case 100: case 118: case 115: case 119: case 109: case 77: case 110: case 111:
            return raw_uint(s, p, end, tag == 100 || tag == 118 ? 2 :
                tag == 109 || tag == 77 || tag == 111 ? 4 : 1, &n) && raw_payload(s, p, end, tag, n);
        case 73: case 75:
            return raw_take(s, p, end, tag == 73 ? 2 : 3) && debit(s, FACTS, 1) && debit(s, TEXT, 1020);
        case 82: return raw_take(s, p, end, 1); /* source decoder owns cache-ref validity */
        case 101: case 102: case 103: case 88: case 89: case 90: case 114: case 120:
            return raw_external(s, p, end, tag, depth);
        case 104: case 105:
            return raw_uint(s, p, end, tag == 104 ? 1 : 4, &n) &&
                debit(s, WORDS, n ? n + 1 : 0) && raw_many(s, p, end, n, depth + 1);
        case 107:
            return raw_uint(s, p, end, 2, &n) && raw_take(s, p, end, n) &&
                debit(s, WORDS, 2 * n) && debit(s, TERMS, n);
        case 108:
            return raw_uint(s, p, end, 4, &n) && debit(s, WORDS, 2 * n) &&
                raw_many(s, p, end, n + 1, depth + 1);
        case 116:
            return raw_uint(s, p, end, 4, &n) && raw_map(s, p, end, n, depth);
        case 113: return debit(s, FACTS, 1) && raw_many(s, p, end, 3, depth + 1);
        case 112:
            /* OTP deliberately ignores NEW_FUN's unreliable Size field. */
            return raw_take(s, p, end, 4 + 1 + 16 + 4) && raw_uint(s, p, end, 4, &n) &&
                n <= 255 && debit(s, WORDS, ERL_FUN_SIZE + n) && debit(s, FACTS, 1) &&
                raw_many(s, p, end, 4 + n, depth + 1);
        case 121:
            if (!raw_take(s, p, end, 4)) return 0;
            continue;
        default: s->refusal = "invalid_external_term"; return 0;
        }
    }
}
static ERL_NIF_TERM refused(struct capture_state *s) {
    if (s->refusal) for (unsigned i = 0; i < 8; i++) if (!strcmp(s->refusal, cap_names[i]))
        return enif_make_tuple2(s->env, atom(s->env, "error"),
            enif_make_tuple2(s->env, atom(s->env, "limit"), atom(s->env, cap_names[i])));
    return error(s->env, s->refusal ? s->refusal : "invalid_term");
}
static ERL_NIF_TERM owner_refused(struct capture_state *s, uint64_t index) {
    if (s->refusal && !strcmp(s->refusal, "source_anchor_required"))
        return enif_make_tuple2(s->env, atom(s->env, "error"),
            enif_make_tuple2(s->env, atom(s->env, "source_anchor_required"), enif_make_uint64(s->env, index)));
    return refused(s);
}
static ERL_NIF_TERM preflight(struct capture_state *s, ERL_NIF_TERM donors) {
    ERL_NIF_TERM item, tail = donors;
    uint64_t i = 0;
    while (enif_get_list_cell(s->env, tail, &item, &tail)) {
        s->emitting = 0; /* Permission never escapes to a following donor. */
        if (enif_is_map(s->env, item) && !anchor_donor(s, item, &item)) return owner_refused(s, i);
        if (!is_bitstring(item)) return error(s->env, "invalid_donors");
        const byte *base; Uint offset, bits;
        ERTS_GET_BITSTRING(item, base, offset, bits);
        if ((offset | bits) & 7) return error(s->env, "unaligned_donor");
        if (!debit(s, DONOR, bits / 8)) return refused(s);
        const byte *p = base + offset / 8, *end = p + bits / 8;
        uint64_t version;
        if (!raw_uint(s, &p, end, 1, &version) || version != 131 ||
            !raw_term(s, &p, end, 0) || p != end) {
            if (s->refusal && !strcmp(s->refusal, "source_anchor_required"))
                return enif_make_tuple2(s->env, atom(s->env, "error"),
                    enif_make_tuple2(s->env, atom(s->env, "source_anchor_required"), enif_make_uint64(s->env, i)));
            if (s->refusal && strcmp(s->refusal, "invalid_external_term")) return refused(s);
            return enif_make_tuple2(s->env, atom(s->env, "error"), enif_make_tuple3(s->env,
                atom(s->env, "source_decode"), enif_make_uint64(s->env, i), atom(s->env, "invalid_external_term")));
        }
        s->emitting = 0;
        /* B2TContext is private to external.c. Bound its exact field roster
         * by SUMMING all three union members, with each scalar rounded to a
         * word: main11, size6, decode6, uncompress2. No copied C type/layout.
         * Include a possible trapped context binary and its live ref. */
        uint64_t context = 25 * 8 + sizeof(ErtsBinary2TermState) + sizeof(ErtsBlockHashState) +
            sizeof(ErtsHeapFactory) + sizeof(ErtsPStack) + sizeof(z_stream);
        if (!debit(s, PINS, sizeof(ErtsMagicBinary) + context) ||
            !debit(s, WORDS, ERTS_MAGIC_REF_THING_SIZE)) return refused(s);
        i++;
    }
    if (!enif_is_empty_list(s->env, tail)) return error(s->env, "invalid_donors");
    /* Term and comparison stacks grow geometrically; retain old and new
     * buffers, and allow two slots per semantic term in each of both walks.
     * C/compiler stacks, allocator carriers and VM globals are not RSS caps. */
    if (!debit(s, WORDS, 3 + 12 * s->used[TERMS]) ||
        !debit(s, PINS, s->used[WORDS] * 8)) return refused(s);
    const char *keys[] = {"decoded_words", "pinned_bytes"};
    ERL_NIF_TERM values[] = {enif_make_uint64(s->env, s->used[WORDS]), enif_make_uint64(s->env, s->used[PINS])};
    return enif_make_tuple2(s->env, atom(s->env, "ok"), map(s->env, keys, values, 2));
}
