#pragma once
#include <stdint.h>

typedef struct CodaVal CodaVal;

typedef enum {
    CODA_INT = 0,
    CODA_STR,
    CODA_CLOSURE,
    CODA_TAG,
    CODA_RECORD,
    CODA_LIST,
} CodaKind;

typedef CodaVal* (*CodaFn)(CodaVal**, CodaVal**, int32_t);

struct CodaVal {
    CodaKind kind;
    int32_t rc;
    union {
        int64_t int_val;
        char *str_val;
        struct { CodaFn fn; CodaVal **caps; int32_t ncaps; } closure;
        struct { const char *name; CodaVal *payload; } tag;
        struct { const char **keys; CodaVal **vals; int32_t nfields; } record;
        struct { CodaVal **items; int32_t len; } list;
    };
};

CodaVal* coda_mk_int(int64_t n);
CodaVal* coda_mk_str(const char *s);
CodaVal* coda_mk_closure(CodaFn fn, CodaVal **caps, int32_t ncaps);
CodaVal* coda_mk_tag(const char *name, CodaVal *payload);
CodaVal* coda_mk_unit(void);
CodaVal* coda_mk_record(const char **keys, CodaVal **vals, int32_t nfields);
CodaVal* coda_mk_list(CodaVal **items, int32_t len);

CodaVal* coda_apply(CodaVal *fn, CodaVal **args, int32_t nargs);
CodaVal* coda_field_get(CodaVal *rec, const char *field);
const char* coda_tag_name(CodaVal *v);
CodaVal* coda_tag_payload(CodaVal *v);
int coda_str_eq(const char *a, const char *b);

CodaVal* coda_add(CodaVal *a, CodaVal *b);
CodaVal* coda_sub(CodaVal *a, CodaVal *b);
CodaVal* coda_mul(CodaVal *a, CodaVal *b);
CodaVal* coda_str_concat(CodaVal *a, CodaVal *b);
CodaVal* coda_eq(CodaVal *a, CodaVal *b);
CodaVal* coda_fix(CodaVal *f);
CodaVal* coda_fix_tail_call(CodaVal *fn, CodaVal **args, int32_t nargs);

CodaVal* coda_cons(CodaVal *x, CodaVal *xs);
CodaVal* coda_append(CodaVal *xs, CodaVal *ys);
CodaVal* coda_head(CodaVal *xs);
CodaVal* coda_tail(CodaVal *xs);
CodaVal* coda_len(CodaVal *xs);
CodaVal* coda_map(CodaVal *f, CodaVal *xs);
CodaVal* coda_fold(CodaVal *f, CodaVal *init, CodaVal *xs);
CodaVal* coda_list_of(CodaVal *n, CodaVal *v);
CodaVal* coda_list_init(CodaVal *n, CodaVal *f);

void coda_retain(CodaVal *v);
void coda_release(CodaVal *v);

void coda_print_val(CodaVal *v);
CodaVal* coda_main(void);
