----------------------------------------------------------------------------------------------------
-- Module Initialization
----------------------------------------------------------------------------------------------------

function Init()
	local m = NewWebsiteModule()
	m.ID               = '4a7b92138e5c46dc9f0e1f7d8c5b3a12'
	m.Name             = 'ScanVF'
	m.RootURL          = 'https://www.scan-vf.net'
	m.Category         = 'French'
	m.OnGetNameAndLink = 'GetNameAndLink'
	m.OnGetInfo        = 'GetInfo'
	m.OnGetPageNumber  = 'GetPageNumber'
end

----------------------------------------------------------------------------------------------------
-- Event Functions
----------------------------------------------------------------------------------------------------

-- Get links and names from the manga list of the current website.
function GetNameAndLink()
	if not HTTP.GET(MODULE.RootURL .. '/manga-list') then return net_problem end

	local x = CreateTXQuery(HTTP.Document)
	for v in x.XPath('//div[contains(@class, "media-body")]//a').Get() do
		local href = v.GetAttribute('href')
		local name = v.GetText()
		if href ~= '' and not href:find('/chapitre-') and not href:find('/manga-list') then
			LINKS.Add(MaybeFillHost(MODULE.RootURL, href))
			NAMES.Add(name)
		end
	end

	return no_error
end

-- Get info and chapter list for the current manga.
function GetInfo()
	MANGAINFO.URL = MaybeFillHost(MODULE.RootURL, URL)
	if not HTTP.GET(MANGAINFO.URL) then return net_problem end

	local x = CreateTXQuery(HTTP.Document)
	MANGAINFO.Title     = x.XPathString('//h2[1]')
	MANGAINFO.CoverLink = MaybeFillHost(MODULE.RootURL, x.XPathString('//img[contains(@src, "cover")]/@src'))
	MANGAINFO.Summary   = x.XPathString('//div[contains(@class, "well")]/p[1]')

	for v in x.XPath('//h5[contains(@class, "chapter-title-rtl")]/a').Get() do
		local href = v.GetAttribute('href')
		local title = v.GetText()
		if href ~= '' then
			MANGAINFO.ChapterLinks.Add(MaybeFillHost(MODULE.RootURL, href))
			MANGAINFO.ChapterNames.Add(title)
		end
	end
	MANGAINFO.ChapterLinks.Reverse(); MANGAINFO.ChapterNames.Reverse()

	return no_error
end

-- Get the page count for the current chapter.
function GetPageNumber()
	local u = MaybeFillHost(MODULE.RootURL, URL)
	if not HTTP.GET(u) then return net_problem end

	local x = CreateTXQuery(HTTP.Document)
	for v in x.XPath('//img[contains(@class, "img-responsive")]').Get() do
		local src = v.GetAttribute('data-src')
		if src == '' then
			src = v.GetAttribute('src')
		end
		src = src:match('^%s*(.-)%s*$')
		if src ~= '' and not src:find('data:image') then
			TASK.PageLinks.Add(MaybeFillHost(MODULE.RootURL, src))
		end
	end

	return no_error
end
