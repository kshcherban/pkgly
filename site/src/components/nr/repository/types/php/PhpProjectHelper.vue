<template>
  <div class="php-repository-helper">
    <p>
      Composer V2 packages are stored by vendor, package, and version. Upload ZIP archives to
      <code>/repositories/&lt;storage&gt;/&lt;repository&gt;/dist/&lt;vendor&gt;/&lt;package&gt;/&lt;version&gt;.zip</code>.
    </p>
    <p>
      Clients fetch metadata from <code>/p2/%package%.json</code> and download the dist from the same
      path; no manual <code>packages.json</code> needed.
    </p>
  </div>
</template>
