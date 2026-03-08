#include "runtime.h"
#include <stdlib.h>
#include <string.h>
#include <stdio.h>

#ifdef CODA_TRACK_ALLOCS
static int64_t coda_live_vals = 0;
#endif

static CodaVal* alloc_val(CodaKind kind) {
    CodaVal *v = malloc(sizeof(CodaVal));
    if (!v) { fprintf(stderr, "oom\n"); exit(1); }
    v->kind = kind;
    v->rc = 1;
#ifdef CODA_TRACK_ALLOCS
    coda_live_vals++;
#endif
    return v;
}

// Sentinel returned by coda_fix_tail_call when trampolining. rc is never
// modified; coda_retain/coda_release skip it.
static CodaVal trampoline_sentinel_storage = { .kind = CODA_INT, .rc = 1 };
CodaVal *const coda_trampoline_sentinel = &trampoline_sentinel_storage;

// Trampoline state (single-threaded; one bounce slot is enough because nested
// fix calls are sequential — inner completes before outer's tail-call fires).
static int   coda_trampoline_depth = 0;
static CodaVal **coda_bounce_args  = NULL;
static int32_t  coda_bounce_nargs  = 0;

void coda_retain(CodaVal *v) {
    if (v && v != coda_trampoline_sentinel) v->rc++;
}

static void drop_children(CodaVal *v);

void coda_release(CodaVal *v) {
    if (!v || v == coda_trampoline_sentinel) return;
    if (--v->rc > 0) return;
    drop_children(v);
    free(v);
#ifdef CODA_TRACK_ALLOCS
    coda_live_vals--;
#endif
}

static void drop_children(CodaVal *v) {
    switch (v->kind) {
        case CODA_STR: free(v->str_val); break;
        case CODA_CLOSURE:
            for (int i = 0; i < v->closure.ncaps; i++)
                coda_release(v->closure.caps[i]);
            free(v->closure.caps);
            break;
        case CODA_TAG:
            coda_release(v->tag.payload);
            break;
        case CODA_RECORD:
            for (int i = 0; i < v->record.nfields; i++)
                coda_release(v->record.vals[i]);
            free(v->record.keys);
            free(v->record.vals);
            break;
        case CODA_LIST:
            for (int i = 0; i < v->list.len; i++)
                coda_release(v->list.items[i]);
            free(v->list.items);
            break;
        default: break;
    }
}

CodaVal* coda_mk_int(int64_t n) {
    CodaVal *v = alloc_val(CODA_INT);
    v->int_val = n;
    return v;
}

CodaVal* coda_mk_str(const char *s) {
    CodaVal *v = alloc_val(CODA_STR);
    v->str_val = strdup(s);
    return v;
}

CodaVal* coda_mk_closure(CodaFn fn, CodaVal **caps, int32_t ncaps) {
    CodaVal *v = alloc_val(CODA_CLOSURE);
    v->closure.fn = fn;
    v->closure.ncaps = ncaps;
    if (ncaps > 0 && caps) {
        v->closure.caps = malloc(ncaps * sizeof(CodaVal*));
        memcpy(v->closure.caps, caps, ncaps * sizeof(CodaVal*));
        for (int i = 0; i < ncaps; i++) coda_retain(caps[i]);
    } else {
        v->closure.caps = NULL;
    }
    return v;
}

CodaVal* coda_mk_tag(const char *name, CodaVal *payload) {
    CodaVal *v = alloc_val(CODA_TAG);
    v->tag.name = name;
    coda_retain(payload);
    v->tag.payload = payload;
    return v;
}

CodaVal* coda_mk_unit(void) {
    return coda_mk_record(NULL, NULL, 0);
}

CodaVal* coda_mk_record(const char **keys, CodaVal **vals, int32_t nfields) {
    CodaVal *v = alloc_val(CODA_RECORD);
    v->record.nfields = nfields;
    if (nfields > 0) {
        v->record.keys = malloc(nfields * sizeof(char*));
        v->record.vals = malloc(nfields * sizeof(CodaVal*));
        memcpy(v->record.keys, keys, nfields * sizeof(char*));
        memcpy(v->record.vals, vals, nfields * sizeof(CodaVal*));
        for (int i = 0; i < nfields; i++) coda_retain(vals[i]);
    } else {
        v->record.keys = NULL;
        v->record.vals = NULL;
    }
    return v;
}

CodaVal* coda_mk_list(CodaVal **items, int32_t len) {
    CodaVal *v = alloc_val(CODA_LIST);
    v->list.len = len;
    if (len > 0 && items) {
        v->list.items = malloc(len * sizeof(CodaVal*));
        memcpy(v->list.items, items, len * sizeof(CodaVal*));
        for (int i = 0; i < len; i++) coda_retain(items[i]);
    } else {
        v->list.items = NULL;
    }
    return v;
}

CodaVal* coda_apply(CodaVal *fn, CodaVal **args, int32_t nargs) {
    if (!fn || fn->kind != CODA_CLOSURE) {
        fprintf(stderr, "runtime error: apply: not a closure\n");
        exit(1);
    }
    return fn->closure.fn(fn->closure.caps, args, nargs);
}

CodaVal* coda_field_get(CodaVal *rec, const char *field) {
    if (!rec || rec->kind != CODA_RECORD) {
        fprintf(stderr, "runtime error: field_get: not a record\n");
        exit(1);
    }
    for (int i = 0; i < rec->record.nfields; i++) {
        if (strcmp(rec->record.keys[i], field) == 0)
            return rec->record.vals[i];
    }
    fprintf(stderr, "runtime error: no field '%s'\n", field);
    exit(1);
}

const char* coda_tag_name(CodaVal *v) {
    if (!v || v->kind != CODA_TAG) {
        fprintf(stderr, "runtime error: tag_name: not a tag\n");
        exit(1);
    }
    return v->tag.name;
}

CodaVal* coda_tag_payload(CodaVal *v) {
    if (!v || v->kind != CODA_TAG) {
        fprintf(stderr, "runtime error: tag_payload: not a tag\n");
        exit(1);
    }
    return v->tag.payload;
}

int coda_str_eq(const char *a, const char *b) {
    return strcmp(a, b) == 0;
}

CodaVal* coda_add(CodaVal *a, CodaVal *b) {
    return coda_mk_int(a->int_val + b->int_val);
}

CodaVal* coda_sub(CodaVal *a, CodaVal *b) {
    return coda_mk_int(a->int_val - b->int_val);
}

CodaVal* coda_mul(CodaVal *a, CodaVal *b) {
    return coda_mk_int(a->int_val * b->int_val);
}

CodaVal* coda_str_concat(CodaVal *a, CodaVal *b) {
    if (a->kind != CODA_STR || b->kind != CODA_STR) {
        fprintf(stderr, "runtime error: ++ requires strings\n"); exit(1);
    }
    size_t la = strlen(a->str_val), lb = strlen(b->str_val);
    char *s = malloc(la + lb + 1);
    memcpy(s, a->str_val, la);
    memcpy(s + la, b->str_val, lb);
    s[la + lb] = '\0';
    CodaVal *v = alloc_val(CODA_STR);
    v->str_val = s;
    return v;
}

static int val_eq(CodaVal *a, CodaVal *b) {
    if (a->kind != b->kind) return 0;
    switch (a->kind) {
        case CODA_INT: return a->int_val == b->int_val;
        case CODA_STR: return strcmp(a->str_val, b->str_val) == 0;
        case CODA_TAG:
            return strcmp(a->tag.name, b->tag.name) == 0 && val_eq(a->tag.payload, b->tag.payload);
        case CODA_RECORD:
            if (a->record.nfields != b->record.nfields) return 0;
            for (int i = 0; i < a->record.nfields; i++) {
                if (strcmp(a->record.keys[i], b->record.keys[i]) != 0) return 0;
                if (!val_eq(a->record.vals[i], b->record.vals[i])) return 0;
            }
            return 1;
        case CODA_LIST:
            if (a->list.len != b->list.len) return 0;
            for (int i = 0; i < a->list.len; i++)
                if (!val_eq(a->list.items[i], b->list.items[i])) return 0;
            return 1;
        default: return 0;
    }
}

CodaVal* coda_eq(CodaVal *a, CodaVal *b) {
    CodaVal *unit = coda_mk_unit();
    CodaVal *tag = coda_mk_tag(val_eq(a, b) ? "True" : "False", unit);
    coda_release(unit);
    return tag;
}

static CodaVal* fix_shim(CodaVal **caps, CodaVal **args, int32_t nargs) {
    CodaVal *f = caps[0];
    CodaVal **cur_args  = args;
    int32_t  cur_nargs  = nargs;
    int      owns_args  = 0;

    while (1) {
        CodaVal *self    = coda_fix(f);
        CodaVal *stepped = coda_apply(f, &self, 1);
        coda_release(self);

        coda_trampoline_depth++;
        CodaVal *result = coda_apply(stepped, cur_args, cur_nargs);
        coda_trampoline_depth--;
        coda_release(stepped);

        if (owns_args) {
            for (int i = 0; i < cur_nargs; i++) coda_release(cur_args[i]);
            free(cur_args);
        }

        if (result == coda_trampoline_sentinel) {
            cur_args   = coda_bounce_args;
            cur_nargs  = coda_bounce_nargs;
            owns_args  = 1;
            coda_bounce_args = NULL;
        } else {
            return result;
        }
    }
}

// Called by generated code in tail position instead of coda_apply.
// When inside a fix_shim trampoline and fn IS a fix_shim closure, store the
// new args in the bounce slot and return the sentinel so fix_shim can loop
// without growing the C stack.  Otherwise behaves identically to coda_apply.
CodaVal* coda_fix_tail_call(CodaVal *fn, CodaVal **args, int32_t nargs) {
    if (coda_trampoline_depth > 0 &&
        fn && fn->kind == CODA_CLOSURE && fn->closure.fn == fix_shim) {
        CodaVal **new_args = malloc(nargs * sizeof(CodaVal*));
        for (int i = 0; i < nargs; i++) {
            new_args[i] = args[i];
            coda_retain(new_args[i]);
        }
        coda_bounce_args  = new_args;
        coda_bounce_nargs = nargs;
        return coda_trampoline_sentinel;
    }
    return coda_apply(fn, args, nargs);
}

CodaVal* coda_fix(CodaVal *f) {
    return coda_mk_closure(fix_shim, &f, 1);
}

CodaVal* coda_cons(CodaVal *x, CodaVal *xs) {
    if (xs->kind != CODA_LIST) { fprintf(stderr, "runtime error: cons: not a list\n"); exit(1); }
    int32_t n = xs->list.len;
    CodaVal **items = malloc((n + 1) * sizeof(CodaVal*));
    items[0] = x;
    if (n > 0) memcpy(items + 1, xs->list.items, n * sizeof(CodaVal*));
    CodaVal *v = alloc_val(CODA_LIST);
    v->list.items = items;
    v->list.len = n + 1;
    // Retain all items stored in the new list
    for (int i = 0; i < n + 1; i++) coda_retain(items[i]);
    return v;
}

CodaVal* coda_append(CodaVal *xs, CodaVal *ys) {
    if (xs->kind != CODA_LIST || ys->kind != CODA_LIST) {
        fprintf(stderr, "runtime error: <>: not a list\n"); exit(1);
    }
    int32_t nx = xs->list.len, ny = ys->list.len;
    int32_t total = nx + ny;
    CodaVal **items = malloc(total * sizeof(CodaVal*));
    if (nx > 0) memcpy(items, xs->list.items, nx * sizeof(CodaVal*));
    if (ny > 0) memcpy(items + nx, ys->list.items, ny * sizeof(CodaVal*));
    CodaVal *v = alloc_val(CODA_LIST);
    v->list.items = items;
    v->list.len = total;
    // Retain all items stored in the new list
    for (int i = 0; i < total; i++) coda_retain(items[i]);
    return v;
}

CodaVal* coda_head(CodaVal *xs) {
    if (xs->kind != CODA_LIST) { fprintf(stderr, "runtime error: head: not a list\n"); exit(1); }
    if (xs->list.len == 0) {
        CodaVal *unit = coda_mk_unit();
        CodaVal *tag = coda_mk_tag("None", unit);
        coda_release(unit);
        return tag;
    }
    return coda_mk_tag("Some", xs->list.items[0]);
}

CodaVal* coda_tail(CodaVal *xs) {
    if (xs->kind != CODA_LIST) { fprintf(stderr, "runtime error: tail: not a list\n"); exit(1); }
    if (xs->list.len == 0) {
        CodaVal *unit = coda_mk_unit();
        CodaVal *tag = coda_mk_tag("None", unit);
        coda_release(unit);
        return tag;
    }
    int32_t n = xs->list.len - 1;
    CodaVal *tail_list = coda_mk_list(xs->list.items + 1, n);
    CodaVal *tag = coda_mk_tag("Some", tail_list);
    coda_release(tail_list);
    return tag;
}

CodaVal* coda_len(CodaVal *xs) {
    if (xs->kind != CODA_LIST) { fprintf(stderr, "runtime error: len: not a list\n"); exit(1); }
    return coda_mk_int(xs->list.len);
}

CodaVal* coda_map(CodaVal *f, CodaVal *xs) {
    if (xs->kind != CODA_LIST) { fprintf(stderr, "runtime error: map: not a list\n"); exit(1); }
    int32_t n = xs->list.len;
    CodaVal **items = malloc(n * sizeof(CodaVal*));
    for (int i = 0; i < n; i++)
        items[i] = coda_apply(f, &xs->list.items[i], 1);
    CodaVal *result = coda_mk_list(items, n);
    // coda_mk_list retains all items; release our refs (rc=1 from coda_apply)
    for (int i = 0; i < n; i++) coda_release(items[i]);
    free(items);
    return result;
}

CodaVal* coda_fold(CodaVal *f, CodaVal *init, CodaVal *xs) {
    if (xs->kind != CODA_LIST) { fprintf(stderr, "runtime error: fold: not a list\n"); exit(1); }
    CodaVal *acc = init;
    coda_retain(acc); // acc is always owned; init is borrowed
    for (int i = 0; i < xs->list.len; i++) {
        CodaVal *fargs[2] = { acc, xs->list.items[i] };
        CodaVal *next = coda_apply(f, fargs, 2);
        coda_release(acc);
        acc = next;
    }
    return acc; // caller takes ownership
}

CodaVal* coda_list_of(CodaVal *n_val, CodaVal *v) {
    int32_t n = (int32_t)n_val->int_val;
    CodaVal **items = malloc(n * sizeof(CodaVal*));
    for (int i = 0; i < n; i++) items[i] = v;
    CodaVal *result = coda_mk_list(items, n);
    free(items);
    return result;
}

CodaVal* coda_list_init(CodaVal *n_val, CodaVal *f) {
    int32_t n = (int32_t)n_val->int_val;
    CodaVal **items = malloc(n * sizeof(CodaVal*));
    for (int i = 0; i < n; i++) {
        CodaVal *idx = coda_mk_int(i);
        items[i] = coda_apply(f, &idx, 1);
        coda_release(idx); // idx was only needed for the apply call
    }
    CodaVal *result = coda_mk_list(items, n);
    // coda_mk_list retains all items; release our refs (rc=1 from coda_apply)
    for (int i = 0; i < n; i++) coda_release(items[i]);
    free(items);
    return result;
}

static void print_inner(CodaVal *v) {
    switch (v->kind) {
        case CODA_INT: printf("%ld", (long)v->int_val); break;
        case CODA_STR: printf("%s", v->str_val); break;
        case CODA_CLOSURE: printf("<fn>"); break;
        case CODA_TAG:
            if (v->tag.payload->kind == CODA_RECORD && v->tag.payload->record.nfields == 0) {
                printf("%s", v->tag.name);
            } else {
                printf("%s ", v->tag.name);
                print_inner(v->tag.payload);
            }
            break;
        case CODA_RECORD:
            printf("{");
            for (int i = 0; i < v->record.nfields; i++) {
                if (i > 0) printf(", ");
                printf("%s: ", v->record.keys[i]);
                print_inner(v->record.vals[i]);
            }
            printf("}");
            break;
        case CODA_LIST:
            printf("[");
            for (int i = 0; i < v->list.len; i++) {
                if (i > 0) printf(", ");
                print_inner(v->list.items[i]);
            }
            printf("]");
            break;
    }
}

void coda_print_val(CodaVal *v) {
    print_inner(v);
    printf("\n");
}

// ok(v): task that returns Ok(v)
static CodaVal* task_ok_body(CodaVal **caps, CodaVal **args, int32_t nargs) {
    return coda_mk_tag("Ok", caps[0]);
}
CodaVal* coda_task_ok(CodaVal *v) {
    return coda_mk_closure(task_ok_body, &v, 1);
}

// fail(e): task that returns Err(e)
static CodaVal* task_fail_body(CodaVal **caps, CodaVal **args, int32_t nargs) {
    return coda_mk_tag("Err", caps[0]);
}
CodaVal* coda_task_fail(CodaVal *e) {
    return coda_mk_closure(task_fail_body, &e, 1);
}

// bind(task, f): task that runs task, on Ok applies f to get next task, runs that; on Err propagates
static CodaVal* task_bind_body(CodaVal **caps, CodaVal **args, int32_t nargs) {
    CodaVal *task = caps[0];
    CodaVal *f    = caps[1];
    CodaVal *res  = coda_apply(task, NULL, 0);
    if (coda_str_eq(coda_tag_name(res), "Ok")) {
        CodaVal *v         = coda_tag_payload(res);
        CodaVal *next_task = coda_apply(f, &v, 1);
        coda_release(res);
        CodaVal *final = coda_apply(next_task, NULL, 0);
        coda_release(next_task);
        return final;
    }
    return res; // Err — propagate
}
CodaVal* coda_task_bind(CodaVal *task, CodaVal *f) {
    CodaVal *caps[2] = {task, f};
    return coda_mk_closure(task_bind_body, caps, 2);
}

// catch(task, handler): task that runs task; on Err applies handler to get recovery task, runs that
static CodaVal* task_catch_body(CodaVal **caps, CodaVal **args, int32_t nargs) {
    CodaVal *task    = caps[0];
    CodaVal *handler = caps[1];
    CodaVal *res = coda_apply(task, NULL, 0);
    if (coda_str_eq(coda_tag_name(res), "Ok")) {
        return res;
    }
    CodaVal *e        = coda_tag_payload(res);
    CodaVal *recovery = coda_apply(handler, &e, 1);
    coda_release(res);
    CodaVal *final = coda_apply(recovery, NULL, 0);
    coda_release(recovery);
    return final;
}
CodaVal* coda_task_catch(CodaVal *task, CodaVal *handler) {
    CodaVal *caps[2] = {task, handler};
    return coda_mk_closure(task_catch_body, caps, 2);
}

// print(s): task that prints s to stdout and returns Ok({})
static CodaVal* task_print_body(CodaVal **caps, CodaVal **args, int32_t nargs) {
    printf("%s\n", caps[0]->str_val);
    CodaVal *unit = coda_mk_unit();
    CodaVal *res  = coda_mk_tag("Ok", unit);
    coda_release(unit);
    return res;
}
CodaVal* coda_task_print(CodaVal *s) {
    return coda_mk_closure(task_print_body, &s, 1);
}

// read_line: task that reads a line from stdin
static CodaVal* task_read_line_body(CodaVal **caps, CodaVal **args, int32_t nargs) {
    char buf[4096];
    if (fgets(buf, sizeof(buf), stdin) == NULL) {
        CodaVal *msg    = coda_mk_str("read error");
        CodaVal *io_err = coda_mk_tag("IoErr", msg);
        coda_release(msg);
        CodaVal *res = coda_mk_tag("Err", io_err);
        coda_release(io_err);
        return res;
    }
    size_t len = strlen(buf);
    if (len > 0 && buf[len - 1] == '\n') buf[--len] = '\0';
    CodaVal *s   = coda_mk_str(buf);
    CodaVal *res = coda_mk_tag("Ok", s);
    coda_release(s);
    return res;
}
CodaVal* coda_task_read_line(void) {
    return coda_mk_closure(task_read_line_body, NULL, 0);
}

// Run task to completion: on Ok returns the ok value (caller owns it); on Err exits 1
CodaVal* coda_run_task(CodaVal *task) {
    CodaVal *res = coda_apply(task, NULL, 0);
    if (coda_str_eq(coda_tag_name(res), "Ok")) {
        CodaVal *val = coda_tag_payload(res);
        coda_retain(val);
        coda_release(res);
        return val;
    }
    fprintf(stderr, "task failed\n");
    coda_release(res);
    exit(1);
}

int main(void) {
    CodaVal *result = coda_main();
    coda_print_val(result);
    coda_release(result);
#ifdef CODA_TRACK_ALLOCS
    if (coda_live_vals != 0) {
        fprintf(stderr, "LEAK: %lld CodaVal(s) still live at exit\n", (long long)coda_live_vals);
        return 1;
    }
#endif
    return 0;
}
