#include "include/pdfium/public/fpdf_annot.h"
#include "include/pdfium/public/fpdf_attachment.h"
#include "include/pdfium/public/fpdf_catalog.h"
#include "include/pdfium/public/fpdf_dataavail.h"
#include "include/pdfium/public/fpdf_doc.h"
#include "include/pdfium/public/fpdf_edit.h"
#include "include/pdfium/public/fpdf_ext.h"
#include "include/pdfium/public/fpdf_flatten.h"
#include "include/pdfium/public/fpdf_formfill.h"
#include "include/pdfium/public/fpdf_fwlevent.h"
#include "include/pdfium/public/fpdf_javascript.h"
#include "include/pdfium/public/fpdf_ppo.h"
#include "include/pdfium/public/fpdf_progressive.h"
#include "include/pdfium/public/fpdf_save.h"
#include "include/pdfium/public/fpdf_searchex.h"
#include "include/pdfium/public/fpdf_signature.h"
#include "include/pdfium/public/fpdf_structtree.h"
#include "include/pdfium/public/fpdf_sysfontinfo.h"
#include "include/pdfium/public/fpdf_text.h"
#include "include/pdfium/public/fpdf_thumbnail.h"
#include "include/pdfium/public/fpdf_transformpage.h"
#include "include/pdfium/public/fpdfview.h"

// Some errors might only be defined with specific features enabled, but
// bindings may need all of them, so define them here.
#ifndef FPDF_ERR_XFALOAD
#define FPDF_ERR_XFALOAD 7    // Load XFA error.
#endif

#ifndef FPDF_ERR_XFALAYOUT
#define FPDF_ERR_XFALAYOUT 8  // Layout XFA error.
#endif
