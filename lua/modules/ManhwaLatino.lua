----------------------------------------------------------------------------------------------------
-- Module Initialization
----------------------------------------------------------------------------------------------------

function Init()
	local m = NewWebsiteModule()
	m.ID                       = 'c3a0df4ce34245cab5e5d9d9b67a782e'
	m.Name                     = 'ManhwaLatino'
	m.RootURL                  = 'https://manhwa-latino.com'
	m.Category                 = 'Spanish'
	m.OnGetNameAndLink         = 'GetNameAndLink'
	m.OnGetInfo                = 'GetInfo'
	m.OnGetPageNumber          = 'GetPageNumber'
	m.OnBeforeDownloadImage    = 'BeforeDownloadImage'
end

----------------------------------------------------------------------------------------------------
-- Local Constants
----------------------------------------------------------------------------------------------------

DirectoryPagination = '/page/'

----------------------------------------------------------------------------------------------------
-- Event Functions
----------------------------------------------------------------------------------------------------

-- Get links and names from the manga list of the current website.
function GetNameAndLink()
	local x = nil
	if not HTTP.GET(MODULE.RootURL .. DirectoryPagination .. (URL + 1)) then return net_problem end

	x = CreateTXQuery(HTTP.Document)
	if x.XPathString('//div[contains(@class, "post-title")]/h3/a') == '' then return no_error end
	x.XPathHREFAll('//div[contains(@class, "post-title")]/h3/a', LINKS, NAMES)
	UPDATELIST.CurrentDirectoryPageNumber = UPDATELIST.CurrentDirectoryPageNumber + 1

	return no_error
end

-- Get info and chapter list for current manga.
function GetInfo()
	local v, x = nil
	local u = MaybeFillHost(MODULE.RootURL, URL)

	if not HTTP.GET(u) then return net_problem end

	x = CreateTXQuery(HTTP.Document)
	MANGAINFO.Title     = x.XPathString('//meta[@property="og:title"]/@content')
	MANGAINFO.CoverLink = x.XPathString('//div[@class="summary_image"]//img/@data-src')
	MANGAINFO.Authors   = x.XPathStringAll('//div[@class="author-content"]/a')
	MANGAINFO.Artists   = x.XPathStringAll('//div[@class="artist-content"]/a')
	MANGAINFO.Genres    = x.XPathStringAll('//div[@class="genres-content"]/a')
	MANGAINFO.Status    = MangaInfoStatusIfPos(x.XPathString('//div[@class="summary-heading" and contains(h5, "Estado")]/following-sibling::div'), 'Publicandose', 'Finalizó')
	MANGAINFO.Summary   = x.XPathString('//div[@class="summary-container"]/p')

	for v in x.XPath('//div[@class="mini-letters"]/a').Get() do
		MANGAINFO.ChapterLinks.Add(v.GetAttribute('href'))
		MANGAINFO.ChapterNames.Add(x.XPathString('h4', v))
	end
	MANGAINFO.ChapterLinks.Reverse(); MANGAINFO.ChapterNames.Reverse()

	HTTP.Reset()
	HTTP.Headers.Values['Referer'] = MANGAINFO.URL

	return no_error
end

-- Get the page count for the current chapter.
function GetPageNumber()
	local u = MaybeFillHost(MODULE.RootURL, URL)
	if string.find(u, 'style=list', 1, true) == nil then u = string.gsub(u, '?style=paged', '') .. '?style=list' end

	if not HTTP.GET(u) then return net_problem end

	CreateTXQuery(HTTP.Document).XPathStringAll('//div[contains(@class, "page-break")]/img/@data-src', TASK.PageLinks)

	return no_error
end

-- Prepare the URL, http header and/or http cookies before downloading an image.
function BeforeDownloadImage()
	HTTP.Headers.Values['Referer'] = MODULE.RootURL

	return true
end
