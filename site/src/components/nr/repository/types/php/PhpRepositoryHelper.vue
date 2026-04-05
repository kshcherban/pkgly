<template>
  <section>
    <h3>Composer Repository</h3>
    <p>
      Composer V2 metadata lives under <code>/repositories/&lt;storage&gt;/&lt;repo&gt;/p2/%package%.json</code>.
      Dist files live under <code>/repositories/&lt;storage&gt;/&lt;repo&gt;/dist/&lt;vendor&gt;/&lt;package&gt;/&lt;version&gt;.zip</code>.
    </p>
    <p>
      Upload with an authenticated <code>PUT</code>/<code>POST</code> to the dist path; Pkgly
      extracts <code>composer.json</code>, validates name/version, rewrites dist URLs back to Pkgly,
      and indexes packages for search.
    </p>
  </section>
</template>
