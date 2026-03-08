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
    CODA_FLOAT,
    CODA_TENSOR,
} CodaKind;

typedef CodaVal* (*CodaFn)(CodaVal**, CodaVal**, int32_t);

struct CodaVal {
    CodaKind kind;
    int32_t rc;
    union {
        int64_t int_val;
        double  float_val;
        char *str_val;
        struct { CodaFn fn; CodaVal **caps; int32_t ncaps; } closure;
        struct { const char *name; CodaVal *payload; } tag;
        struct { const char **keys; CodaVal **vals; int32_t nfields; } record;
        struct { CodaVal **items; int32_t len; } list;
        struct { int64_t rows; int64_t cols; double *data; } tensor;
    };
};

CodaVal* coda_mk_int(int64_t n);
int64_t  coda_unbox_int(CodaVal *v);
CodaVal* coda_mk_float(double v);
double   coda_unbox_float(CodaVal *v);
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

CodaVal* coda_task_ok(CodaVal *v);
CodaVal* coda_task_fail(CodaVal *e);
CodaVal* coda_task_bind(CodaVal *task, CodaVal *f);
CodaVal* coda_task_catch(CodaVal *task, CodaVal *handler);
CodaVal* coda_task_print(CodaVal *s);
CodaVal* coda_task_read_line(void);
CodaVal* coda_run_task(CodaVal *task);

/* Tensor operations */
CodaVal* coda_mk_tensor(int64_t rows, int64_t cols);
CodaVal* coda_mk_tensor_ones(int64_t rows, int64_t cols);
CodaVal* coda_tensor_matmul(CodaVal *a, CodaVal *b);
CodaVal* coda_tensor_add(CodaVal *a, CodaVal *b);
CodaVal* coda_tensor_scale(CodaVal *t, double scalar);
CodaVal* coda_tensor_reshape(int64_t rows, int64_t cols, CodaVal *t);
CodaVal* coda_tensor_get(CodaVal *t, int64_t i, int64_t j);
CodaVal* coda_tensor_set(CodaVal *t, int64_t i, int64_t j, CodaVal *val);
int64_t  coda_tensor_rows(CodaVal *t);
int64_t  coda_tensor_cols(CodaVal *t);
